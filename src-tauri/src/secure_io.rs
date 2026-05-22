//! User-only file helpers for secrets at rest.
//!
//! On **Unix**, the helpers explicitly chmod the file/dir to `0o600` /
//! `0o700` so other local users on the same machine cannot read them.
//!
//! On **Windows** there is no chmod equivalent — the file inherits the
//! ACL of its containing directory. We rely on Tauri placing our state
//! inside `%LOCALAPPDATA%\<identifier>\`, which the OS already locks to
//! the user's SID via NTFS inheritance. The `#[cfg(unix)]` blocks below
//! are therefore intentionally Windows no-ops, not missing coverage.

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::Path;

/// Read a file, refusing to load more than `max_bytes`. Unbounded
/// `fs::read_to_string` on attacker-controlled JSON is a denial-of-
/// service primitive (a 4 GiB file gets loaded into RAM); every JSON
/// state file we own should route through this. Returns
/// `ErrorKind::InvalidData` when the file is too large so callers can
/// distinguish from missing/permission errors.
pub fn read_capped(path: &Path, max_bytes: u64) -> io::Result<String> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();
    if size > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{}: file is {size} bytes, exceeds cap of {max_bytes}",
                path.display()
            ),
        ));
    }
    let file = std::fs::File::open(path)?;
    let mut buf = String::with_capacity(size as usize);
    file.take(max_bytes).read_to_string(&mut buf)?;
    Ok(buf)
}

pub fn write_user_only(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("write_user_only: path has no parent"))?;
    std::fs::create_dir_all(dir)?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("write_user_only: invalid file name"))?;
    let tmp = dir.join(format!(".{file_name}.tmp"));
    let _ = std::fs::remove_file(&tmp);

    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);

    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

pub fn ensure_user_only_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn tighten_existing_file(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        // `set_permissions` follows symlinks, so a chmod via path would
        // hit whatever the link points at — a privilege manipulation
        // primitive if an attacker can replace a rotated log with a
        // symlink to a sensitive file between rotations and the sweep.
        // Open the file (with `O_NOFOLLOW`) and `fchmod` the fd
        // instead, so the chmod can only ever touch the inode we
        // actually opened.
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let rc = unsafe { libc::fchmod(file.as_raw_fd(), 0o600) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    #[cfg(not(unix))]
    {
        // Windows: `set_permissions` here only toggles the read-only
        // bit, and the symlink-follow concern is Unix-specific. File
        // protection on Windows comes from the inherited ACL of
        // `%LOCALAPPDATA%\<identifier>\` — see the module docstring.
        let _ = path;
    }
    Ok(())
}

pub fn tighten_existing_files_in_dir(dir: &Path) -> io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Skip symlinks even before delegating to `tighten_existing_file`
        // — the inner helper already refuses to follow them, but
        // filtering here keeps the intent visible at the call site
        // and avoids an unnecessary `O_NOFOLLOW` open + error.
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let _ = tighten_existing_file(&entry.path());
    }
    Ok(())
}

/// One iteration of the periodic tighten sweep. Extracted from the
/// `spawn_periodic_dir_tighten` loop so tests can drive a single tick
/// synchronously instead of polling against a `thread::sleep` timer.
pub fn tighten_once(dir: &Path) {
    let _ = tighten_existing_files_in_dir(dir);
}

/// Spawn a detached background thread that re-runs
/// `tighten_existing_files_in_dir(&dir)` every `interval`.
///
/// `tauri_plugin_log` creates rotated log files (`entracte.log.1`,
/// `.log.2`, …) via `OpenOptions` without setting an explicit mode,
/// so on Unix they pick up `0o644` from the process umask — wider
/// than the `0o600` we promise. The startup-only tighten in `lib::run`
/// misses everything created after boot. This periodic sweep closes
/// that gap without coupling the log plugin to our security helpers.
///
/// Returns immediately; the thread runs for the process lifetime.
/// Errors from individual `tighten_existing_file` calls are swallowed
/// inside the helper (matching the startup path).
pub fn spawn_periodic_dir_tighten(dir: std::path::PathBuf, interval: std::time::Duration) {
    std::thread::spawn(move || loop {
        std::thread::sleep(interval);
        tighten_once(&dir);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn read_capped_under_cap_returns_contents() {
        let dir = temp_dir();
        let path = dir.path().join("ok");
        std::fs::write(&path, b"hello").unwrap();
        let got = read_capped(&path, 1024).unwrap();
        assert_eq!(got, "hello");
    }

    #[test]
    fn read_capped_over_cap_errors_with_invalid_data() {
        let dir = temp_dir();
        let path = dir.path().join("big");
        std::fs::write(&path, vec![b'x'; 2048]).unwrap();
        let err = read_capped(&path, 1024).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_capped_missing_returns_not_found() {
        let dir = temp_dir();
        let path = dir.path().join("nope");
        let err = read_capped(&path, 1024).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn write_user_only_creates_at_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.path().join("secret");
        write_user_only(&path, b"hello").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[cfg(unix)]
    #[test]
    fn write_user_only_overwrites_at_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.path().join("secret");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_user_only(&path, b"new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn write_user_only_cleans_stale_tmp() {
        let dir = temp_dir();
        let path = dir.path().join("secret");
        let tmp = dir.path().join(".secret.tmp");
        std::fs::write(&tmp, b"leftover").unwrap();
        write_user_only(&path, b"fresh").unwrap();
        assert!(!tmp.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");
    }

    #[cfg(unix)]
    #[test]
    fn tighten_existing_file_drops_existing_file_to_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.path().join("entracte.log");
        std::fs::write(&path, b"x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        tighten_existing_file(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn tighten_existing_file_does_not_follow_symlink_to_target() {
        // Regression for the symlink-follow chmod primitive:
        // tightening a symlink must not propagate the chmod to the
        // link's target. The target stays at whatever mode it had.
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let target = dir.path().join("real-target");
        std::fs::write(&target, b"sensitive").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let link = dir.path().join("entracte.log.1");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // `tighten_existing_file` on the symlink must not error out
        // the caller (the sweep keeps running) and must not touch the
        // target's mode.
        let _ = tighten_existing_file(&link);
        let target_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            target_mode, 0o644,
            "target mode must be untouched when tightening a symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tighten_existing_files_in_dir_skips_symlink_entries() {
        // Same primitive, but through the directory sweep: a symlink
        // sitting next to real log files must be skipped, not chmodded
        // through to its target. The target lives in a *separate*
        // directory so the sweep would never reach it directly — the
        // only way it could be touched is via following the symlink.
        use std::os::unix::fs::PermissionsExt;
        let log_dir = temp_dir();
        let target_dir = temp_dir();
        let real = log_dir.path().join("entracte.log");
        std::fs::write(&real, b"x").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o644)).unwrap();
        let target = target_dir.path().join("sensitive-target");
        std::fs::write(&target, b"keep me").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let link = log_dir.path().join("entracte.log.1");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        tighten_existing_files_in_dir(log_dir.path()).unwrap();
        let real_mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(real_mode, 0o600, "real file should still be tightened");
        let target_mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            target_mode, 0o644,
            "symlink target must not be chmodded by the sweep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn tighten_existing_files_in_dir_tightens_each_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        for name in ["a.log", "b.log.1", "c.log.2"] {
            let p = dir.path().join(name);
            std::fs::write(&p, b"x").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        // Sub-dir should be skipped (only files).
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        tighten_existing_files_in_dir(dir.path()).unwrap();
        for name in ["a.log", "b.log.1", "c.log.2"] {
            let mode = std::fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{name} should be 0o600");
        }
    }

    #[test]
    fn tighten_existing_files_in_dir_is_noop_when_missing() {
        let dir = temp_dir();
        let missing = dir.path().join("does-not-exist");
        tighten_existing_files_in_dir(&missing).unwrap();
        assert!(!missing.exists());
    }

    #[test]
    fn tighten_existing_file_is_noop_when_missing() {
        let dir = temp_dir();
        let path = dir.path().join("does-not-exist.log");
        tighten_existing_file(&path).unwrap();
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn tighten_once_re_tightens_file_created_after_startup() {
        // Simulates the log-rotation case: a file appears in the dir
        // after the watcher has started, with default permissive perms,
        // and one tighten tick drops it to 0o600. Driving `tighten_once`
        // directly removes the prior reliance on `thread::sleep` timing.
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.path().join("rotated.log.1");
        std::fs::write(&path, b"after-rotation").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        tighten_once(dir.path());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_user_only_dir_locks_existing_dir_to_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_user_only_dir(dir.path()).unwrap();
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    // Cross-platform behavioural tests.
    //
    // The mode-checking tests above only run on Unix because Windows has
    // no chmod equivalent — but the *file operations themselves*
    // (create, overwrite, tmp cleanup, missing-path tolerance) must work
    // on every platform. Without these, Windows CI would only exercise
    // `tighten_existing_file_is_noop_when_missing`, which doesn't touch
    // any of the file-creation logic.

    #[test]
    fn write_user_only_writes_expected_content() {
        let dir = temp_dir();
        let path = dir.path().join("secret");
        write_user_only(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn write_user_only_creates_parent_dir() {
        let dir = temp_dir();
        let nested = dir.path().join("nested").join("deep");
        let path = nested.join("secret");
        assert!(!nested.exists());
        write_user_only(&path, b"x").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn write_user_only_overwrites_existing_file() {
        let dir = temp_dir();
        let path = dir.path().join("secret");
        std::fs::write(&path, b"old").unwrap();
        write_user_only(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn write_user_only_removes_stale_tmp_before_writing() {
        let dir = temp_dir();
        let path = dir.path().join("secret");
        let tmp = dir.path().join(".secret.tmp");
        std::fs::write(&tmp, b"leftover").unwrap();
        write_user_only(&path, b"fresh").unwrap();
        assert!(!tmp.exists(), "stale tmp should be cleaned up");
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");
    }

    #[test]
    fn tighten_existing_file_returns_ok_on_real_file() {
        // On Unix this drops to 0o600 (covered above); on Windows it's a
        // documented no-op. Either way the call must succeed without
        // erroring on a normal user-writable file.
        let dir = temp_dir();
        let path = dir.path().join("file.log");
        std::fs::write(&path, b"x").unwrap();
        tighten_existing_file(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn ensure_user_only_dir_creates_missing_dir() {
        let dir = temp_dir();
        let nested = dir.path().join("new-dir");
        assert!(!nested.exists());
        ensure_user_only_dir(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn ensure_user_only_dir_is_idempotent() {
        let dir = temp_dir();
        ensure_user_only_dir(dir.path()).unwrap();
        ensure_user_only_dir(dir.path()).unwrap();
        ensure_user_only_dir(dir.path()).unwrap();
        assert!(dir.path().exists());
    }

    // Windows-specific test: the helpers must not error on Windows even
    // though they don't touch the DACL. On Windows the protection comes
    // from the file inheriting the ACL of `%LOCALAPPDATA%\<identifier>\`,
    // not from anything this module does — see the module docstring.
    //
    // This test asserts the "no chmod, no problem" contract: the file
    // round-trips through write_user_only + tighten_existing_file and is
    // still readable/writable afterwards by the test process (which is
    // the same SID that will own the file in production).
    #[cfg(windows)]
    #[test]
    fn windows_round_trip_preserves_owner_access() {
        let dir = temp_dir();
        let path = dir.path().join("secret");
        write_user_only(&path, b"original").unwrap();
        // Re-read to confirm the test process can still read it after
        // create_new + rename. (If the rename ever inherited a
        // restrictive ACL without our SID, this would fail with
        // ERROR_ACCESS_DENIED.)
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        tighten_existing_file(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::write(&path, b"rewritten").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"rewritten");
    }

    #[cfg(windows)]
    #[test]
    fn windows_tighten_dir_iterates_without_erroring() {
        let dir = temp_dir();
        for name in ["a.log", "b.log", "c.log"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        tighten_existing_files_in_dir(dir.path()).unwrap();
        for name in ["a.log", "b.log", "c.log"] {
            assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"x");
        }
    }
}
