// Offline issuer for Entracte manual licences.
//
// Usage:
//   cargo run --bin issue-license -- generate
//   cargo run --bin issue-license -- sign \
//       --name "Jane Doe" \
//       --key-file ~/.entracte/license-private.key
//
// `generate` prints a fresh Ed25519 keypair: keep the private key out of
// the repo, paste the public key into `EMBEDDED_PUBLIC_KEY_HEX` in
// src-tauri/src/supporter/manual.rs, then ship a release.
//
// `sign` reads a 32-byte hex-encoded private key (from --key-file, or
// from the ENTRACTE_LICENSE_PRIVATE_KEY env var) and emits one
// `ENT1-...` token bound to the given name.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use entracte_lib::supporter::manual::{sign, ManualLicense};
use zeroize::{Zeroize, Zeroizing};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("generate") => generate(),
        Some("sign") => sign_cmd(&args[1..]),
        Some("--help") | Some("-h") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "issue-license — generate Entracte manual licences offline\n\
\n\
USAGE:\n  \
issue-license generate\n  \
issue-license sign --name <recipient> [--key-file <path>]\n\
\n\
The signing key is read from --key-file, or from the\n\
ENTRACTE_LICENSE_PRIVATE_KEY env var (32 bytes hex-encoded)."
    );
}

fn generate() -> ExitCode {
    let mut seed = Zeroizing::new([0u8; 32]);
    if let Err(e) = getrandom::getrandom(&mut *seed) {
        eprintln!("could not source randomness: {e}");
        return ExitCode::FAILURE;
    }
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();
    // Emit the private key once and warn the user. The hex string is
    // *not* zeroized — printing it to stdout means the bytes leave
    // process memory regardless; the warning is the actual control.
    let mut private_hex = hex::encode(*seed);
    println!("private_key_hex = {private_hex}");
    println!("public_key_hex  = {}", hex::encode(verifying.to_bytes()));
    println!(
        "\n!! Treat private_key_hex like a credential — do not paste it into chat or\n   \
shell history. Save it directly to an encrypted file (1Password, age, etc.).\n\n\
Next steps:\n  \
1. Save private_key_hex somewhere off the repo.\n  \
2. Paste public_key_hex into EMBEDDED_PUBLIC_KEY_HEX in\n     \
src-tauri/src/supporter/manual.rs and ship a release."
    );
    private_hex.zeroize();
    ExitCode::SUCCESS
}

fn sign_cmd(args: &[String]) -> ExitCode {
    let mut name: Option<String> = None;
    let mut key_file: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--name" => {
                name = iter.next().cloned();
            }
            "--key-file" => {
                key_file = iter.next().map(PathBuf::from);
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unexpected argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(name) = name.filter(|n| !n.trim().is_empty()) else {
        eprintln!("--name is required and must not be empty");
        return ExitCode::FAILURE;
    };

    let key_hex: Zeroizing<String> =
        match (key_file, std::env::var("ENTRACTE_LICENSE_PRIVATE_KEY").ok()) {
            (Some(path), _) => {
                if let Err(e) = check_key_file_permissions(&path) {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
                match std::fs::read_to_string(&path) {
                    Ok(s) => Zeroizing::new(s.trim().to_string()),
                    Err(e) => {
                        eprintln!("could not read key file {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                }
            }
            (None, Some(s)) => Zeroizing::new(s.trim().to_string()),
            (None, None) => {
                eprintln!(
                    "no signing key provided: pass --key-file <path> or set\n\
                     ENTRACTE_LICENSE_PRIVATE_KEY to the 32-byte hex-encoded private key"
                );
                return ExitCode::FAILURE;
            }
        };

    let key_bytes: Zeroizing<Vec<u8>> = match hex::decode(key_hex.as_str()) {
        Ok(b) => Zeroizing::new(b),
        Err(e) => {
            eprintln!("private key is not valid hex: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut key_array = Zeroizing::new([0u8; 32]);
    if key_bytes.len() != 32 {
        eprintln!(
            "private key must be 32 bytes ({} provided)",
            key_bytes.len()
        );
        return ExitCode::FAILURE;
    }
    key_array.copy_from_slice(&key_bytes);
    let signing = SigningKey::from_bytes(&key_array);

    let license = ManualLicense {
        name: name.trim().to_string(),
        issued_at: Utc::now(),
    };
    match sign(&signing, &license) {
        Ok(token) => {
            println!("{token}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("failed to sign licence: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Refuse to read a key file that is group/world-readable. The hex
/// encoding is sensitive enough that we want to surface a permissions
/// error instead of silently signing with a leaky-on-disk secret.
fn check_key_file_permissions(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .map_err(|e| format!("could not stat key file {}: {e}", path.display()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "refusing to read key file {} with permissions {:#o}; chmod 600 first",
                path.display(),
                mode
            ));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
