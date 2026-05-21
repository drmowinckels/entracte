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
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        eprintln!("could not source randomness: {e}");
        return ExitCode::FAILURE;
    }
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();
    println!("private_key_hex = {}", hex::encode(seed));
    println!("public_key_hex  = {}", hex::encode(verifying.to_bytes()));
    println!(
        "\nNext steps:\n  \
1. Save private_key_hex somewhere off the repo (1Password / encrypted file).\n  \
2. Paste public_key_hex into EMBEDDED_PUBLIC_KEY_HEX in\n     \
src-tauri/src/supporter/manual.rs and ship a release."
    );
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

    let key_hex = match (key_file, std::env::var("ENTRACTE_LICENSE_PRIVATE_KEY").ok()) {
        (Some(path), _) => match std::fs::read_to_string(&path) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                eprintln!("could not read key file {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        (None, Some(s)) => s.trim().to_string(),
        (None, None) => {
            eprintln!(
                "no signing key provided: pass --key-file <path> or set\n\
                 ENTRACTE_LICENSE_PRIVATE_KEY to the 32-byte hex-encoded private key"
            );
            return ExitCode::FAILURE;
        }
    };

    let key_bytes = match hex::decode(&key_hex) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("private key is not valid hex: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key_array: [u8; 32] = match key_bytes.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("private key must be 32 bytes ({} provided)", key_bytes.len());
            return ExitCode::FAILURE;
        }
    };
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
