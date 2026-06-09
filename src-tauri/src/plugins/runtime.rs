//! The WASM capability sandbox (#156, slice 4).
//!
//! A plugin module is loaded into extism (embedding wasmtime) with **no
//! ambient authority**: WASI is disabled, so the module gets no filesystem,
//! clock, randomness, or network. Its entire outside-world surface is the
//! host functions the host registers — and the host registers a function
//! *only* if the matching capability was granted. A module that imports a
//! host function whose capability wasn't granted **fails to load** (a
//! wasmtime link error): the runtime half of the capability model, paired
//! with the manifest-level `imports`↔grant check in `validate_manifest`.
//!
//! Execution is bounded three ways — a memory cap, a wasmtime fuel cap, and a
//! wall-clock timeout — so a runaway module is trapped, never hangs the host.
//!
//! The host-function **bodies** here are stubs: this slice establishes the
//! sandbox, the capability→host-function ABI, and the enforcement/metering
//! contract. The detector and export slices replace the stubs with real,
//! scope-checked implementations (read the foreground window, POST to the
//! granted origin, …).

// Foundational slice: the sandbox builder + ABI mapping have no in-crate
// caller yet (the detector and export slices consume them per the design
// doc's staging plan), so they read as dead in the non-test build. The
// wat-driven tests exercise them directly. Scoped to this module; removed
// when the first consumer lands.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::Duration;

use extism::{
    CurrentPlugin, Error as ExtismError, Manifest, PluginBuilder, UserData, Val, ValType, Wasm,
};

use super::detect;
use super::manifest::Capability;

/// Per-install context the host functions need beyond the capability scopes
/// themselves. The `detect:file:<path>` scope lives in the capability; the
/// process pattern a `detect:processes` grant matches against comes from the
/// manifest's detect config and is threaded in here.
#[derive(Debug, Clone, Default)]
pub struct SandboxContext {
    pub process_pattern: Option<String>,
}

/// Memory ceiling for a plugin instance, in 64 KiB wasm pages. 64 pages =
/// 4 MiB — generous for a detector/export module, tight enough that a
/// runaway allocation traps quickly.
pub const DEFAULT_MEMORY_MAX_PAGES: u32 = 64;

/// Wall-clock ceiling for a single plugin call. Detectors run on a throttled
/// interval off the scheduler tick, so 250 ms is ample for a real probe
/// while bounding an accidental infinite loop.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(250);

/// wasmtime fuel ceiling per instance — belt to the timeout's braces, so a
/// tight CPU loop traps on fuel even if the timer thread is starved. Roughly
/// one unit per wasm instruction; 5e8 is far above any real probe.
pub const DEFAULT_FUEL: u64 = 500_000_000;

/// The host-function name a capability unlocks in the `extism:host/user`
/// namespace, i.e. the symbol a module imports to use that capability. The
/// sandbox registers exactly these for the granted capabilities.
pub fn host_function_name(cap: &Capability) -> &'static str {
    match cap {
        Capability::DetectForegroundWindow => "host_foreground_window",
        Capability::DetectProcesses => "host_process_running",
        Capability::DetectFile(_) => "host_read_flag",
        Capability::ExportFile(_) => "host_write_file",
        Capability::ExportHttp(_) => "host_http_post",
    }
}

/// The distinct host-function names the granted capabilities unlock, in a
/// stable order with duplicates removed (two `detect:file:<path>` grants map
/// to one `host_read_flag`). Pure — the registration list without a wasm
/// engine.
pub fn host_function_names_for(grants: &[Capability]) -> Vec<&'static str> {
    let mut names = Vec::new();
    for cap in grants {
        let name = host_function_name(cap);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Set the single i64 output of a host function to a boolean (1/0).
fn set_bool(outputs: &mut [Val], value: bool) {
    if let Some(out) = outputs.first_mut() {
        *out = Val::I64(value as i64);
    }
}

/// Placeholder body for host functions whose real implementation lands in a
/// later slice (foreground-window and the export sinks). Returns 0. Its mere
/// presence is still gated by the capability, so registering it doesn't widen
/// the module's reach.
fn host_stub(
    _plugin: &mut CurrentPlugin,
    _inputs: &[Val],
    outputs: &mut [Val],
    _user_data: UserData<()>,
) -> Result<(), ExtismError> {
    set_bool(outputs, false);
    Ok(())
}

/// Register the host function a single capability unlocks, with a body that's
/// scope-checked by construction: the data each closure needs (the process
/// pattern, the granted file path) is captured here from the grant + context,
/// never supplied by the module. All host functions are `() -> i64` booleans
/// in this ABI.
fn register_capability<'a>(
    builder: PluginBuilder<'a>,
    cap: &Capability,
    ctx: &SandboxContext,
) -> PluginBuilder<'a> {
    let name = host_function_name(cap);
    match cap {
        Capability::DetectProcesses => {
            let pattern = ctx.process_pattern.clone().unwrap_or_default();
            builder.with_function(
                name,
                [],
                [ValType::I64],
                UserData::new(pattern),
                |_p, _in, out, ud| {
                    let data = ud.get()?;
                    let pattern = data.lock().unwrap();
                    set_bool(out, detect::process_running(&pattern));
                    Ok(())
                },
            )
        }
        Capability::DetectFile(path) => {
            let path = PathBuf::from(path);
            builder.with_function(
                name,
                [],
                [ValType::I64],
                UserData::new(path),
                |_p, _in, out, ud| {
                    let data = ud.get()?;
                    let path = data.lock().unwrap();
                    set_bool(out, detect::read_flag(&path));
                    Ok(())
                },
            )
        }
        // Foreground-window and the export sinks are stubbed until their
        // slices; registered (gated) but inert.
        Capability::DetectForegroundWindow
        | Capability::ExportFile(_)
        | Capability::ExportHttp(_) => {
            builder.with_function(name, [], [ValType::I64], UserData::default(), host_stub)
        }
    }
}

/// Build a sandboxed plugin from `module` bytes, registering host functions
/// **only** for the granted `capabilities`. WASI is off and memory / fuel /
/// timeout are bounded (see the `DEFAULT_*` consts). Returns a user-facing
/// error if the module fails to compile, link, or instantiate — including the
/// case where it imports a host function whose capability wasn't granted.
///
/// Duplicate host-function names are registered once (the first grant wins),
/// so a plugin with two `detect:file:<path>` grants links cleanly.
pub fn build_sandboxed_plugin(
    module: &[u8],
    capabilities: &[Capability],
    ctx: &SandboxContext,
) -> Result<extism::Plugin, String> {
    let manifest = Manifest::new([Wasm::data(module.to_vec())])
        .with_memory_max(DEFAULT_MEMORY_MAX_PAGES)
        .with_timeout(DEFAULT_TIMEOUT);

    let mut builder = PluginBuilder::new(manifest)
        .with_wasi(false)
        .with_fuel_limit(DEFAULT_FUEL);

    let mut registered: Vec<&str> = Vec::new();
    for cap in capabilities {
        let name = host_function_name(cap);
        if registered.contains(&name) {
            continue;
        }
        registered.push(name);
        builder = register_capability(builder, cap, ctx);
    }

    builder
        .build()
        .map_err(|e| format!("plugin failed to load in the sandbox: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).expect("valid WAT")
    }

    #[test]
    fn host_function_names_dedupe_and_map_per_capability() {
        let grants = vec![
            Capability::DetectForegroundWindow,
            Capability::DetectFile("/a".to_string()),
            Capability::DetectFile("/b".to_string()), // same host fn → one entry
        ];
        assert_eq!(
            host_function_names_for(&grants),
            vec!["host_foreground_window", "host_read_flag"]
        );
    }

    #[test]
    fn host_function_name_covers_every_capability() {
        for cap in [
            Capability::DetectForegroundWindow,
            Capability::DetectProcesses,
            Capability::DetectFile("/p".to_string()),
            Capability::ExportFile("/p".to_string()),
            Capability::ExportHttp("127.0.0.1:8080".to_string()),
        ] {
            assert!(!host_function_name(&cap).is_empty());
        }
    }

    /// A module that calls the named `() -> i64` host function and **traps**
    /// when it returns non-zero, so a test can read the host's boolean answer
    /// through call success (false) vs. error (true) — no extism output
    /// marshalling needed.
    fn trap_if_true_module(host_fn: &str) -> Vec<u8> {
        wasm(&format!(
            r#"(module
                 (import "extism:host/user" "{host_fn}" (func $f (result i64)))
                 (memory (export "memory") 1)
                 (func (export "run") (result i32)
                   (if (i64.ne (call $f) (i64.const 0)) (then unreachable))
                   (i32.const 0)))"#
        ))
    }

    #[test]
    fn loads_a_clean_module_with_no_imports() {
        let module = wasm(
            r#"(module
                 (memory (export "memory") 1)
                 (func (export "run") (result i32) i32.const 0))"#,
        );
        assert!(build_sandboxed_plugin(&module, &[], &SandboxContext::default()).is_ok());
    }

    #[test]
    fn rejects_a_module_importing_an_ungranted_host_function() {
        // Imports host_process_running but no detect:processes grant.
        let module = trap_if_true_module("host_process_running");
        let err = build_sandboxed_plugin(&module, &[], &SandboxContext::default()).unwrap_err();
        assert!(
            err.contains("failed to load"),
            "ungranted import should fail the load, got: {err}"
        );
    }

    #[test]
    fn host_process_running_reports_a_live_process_to_the_module() {
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().unwrap().to_string_lossy().to_string();
        let needle = stem[..stem.len().min(6)].to_string();

        // Pattern matches a live process (this test binary) → host returns
        // true → the module traps → the call errors.
        let module = trap_if_true_module("host_process_running");
        let mut plugin = build_sandboxed_plugin(
            &module,
            &[Capability::DetectProcesses],
            &SandboxContext {
                process_pattern: Some(needle),
            },
        )
        .expect("granted detect:processes builds");
        assert!(
            plugin.call::<&str, &str>("run", "").is_err(),
            "a matching process should be reported true (module traps)"
        );

        // A pattern that matches nothing → host returns false → call succeeds.
        let module = trap_if_true_module("host_process_running");
        let mut plugin = build_sandboxed_plugin(
            &module,
            &[Capability::DetectProcesses],
            &SandboxContext {
                process_pattern: Some("entracte-no-such-process-zzz".to_string()),
            },
        )
        .unwrap();
        assert!(plugin.call::<&str, &str>("run", "").is_ok());
    }

    #[test]
    fn host_read_flag_reports_the_granted_files_truthiness() {
        let dir = crate::test_support::temp_dir();
        let flag = dir.path().join("focus.flag");
        std::fs::write(&flag, "true").unwrap();
        let cap = Capability::DetectFile(flag.display().to_string());

        // Truthy flag → host returns true → module traps → call errors.
        let module = trap_if_true_module("host_read_flag");
        let mut plugin = build_sandboxed_plugin(
            &module,
            std::slice::from_ref(&cap),
            &SandboxContext::default(),
        )
        .expect("granted detect:file builds");
        assert!(plugin.call::<&str, &str>("run", "").is_err());

        // Flip the flag to falsey → host returns false → call succeeds.
        std::fs::write(&flag, "0").unwrap();
        let module = trap_if_true_module("host_read_flag");
        let mut plugin = build_sandboxed_plugin(
            &module,
            std::slice::from_ref(&cap),
            &SandboxContext::default(),
        )
        .unwrap();
        assert!(plugin.call::<&str, &str>("run", "").is_ok());
    }

    #[test]
    fn a_stubbed_capability_registers_an_inert_host_function() {
        // Foreground-window is still a stub: a granted module that calls it
        // links and the stub returns false, so the trap-if-true module's call
        // succeeds.
        let module = trap_if_true_module("host_foreground_window");
        let mut plugin = build_sandboxed_plugin(
            &module,
            &[Capability::DetectForegroundWindow],
            &SandboxContext::default(),
        )
        .expect("granted detect:foreground-window builds");
        assert!(
            plugin.call::<&str, &str>("run", "").is_ok(),
            "the stub should return false (no suppression)"
        );
    }

    #[test]
    fn duplicate_capabilities_register_their_host_function_once() {
        // Two detect:file grants map to one host_read_flag; the second is
        // skipped so the module links cleanly against a single registration.
        let module = trap_if_true_module("host_read_flag");
        let grants = vec![
            Capability::DetectFile("/tmp/a.flag".to_string()),
            Capability::DetectFile("/tmp/b.flag".to_string()),
        ];
        assert!(build_sandboxed_plugin(&module, &grants, &SandboxContext::default()).is_ok());
    }

    #[test]
    fn a_runaway_module_is_trapped_not_hung() {
        // Infinite loop; the wall-clock timeout (and fuel) must abort the
        // call rather than hang the test.
        let module = wasm(
            r#"(module
                 (memory (export "memory") 1)
                 (func (export "run") (result i32)
                   (loop $l (br $l))
                   (i32.const 0)))"#,
        );
        let mut plugin =
            build_sandboxed_plugin(&module, &[], &SandboxContext::default()).expect("module loads");
        let started = std::time::Instant::now();
        let result = plugin.call::<&str, &str>("run", "");
        let elapsed = started.elapsed();
        assert!(result.is_err(), "runaway call must error, not return");
        assert!(
            elapsed < Duration::from_secs(5),
            "metering must abort the runaway promptly"
        );
    }
}
