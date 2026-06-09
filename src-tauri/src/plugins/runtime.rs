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

use std::time::Duration;

use extism::{
    CurrentPlugin, Error as ExtismError, Manifest, PluginBuilder, UserData, Val, ValType, Wasm,
};

use super::manifest::Capability;

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

/// Stub host-function body for this slice: takes one i64, returns 0. Slices 5
/// and 6 replace these with real, scope-checked implementations. Registered
/// only for granted capabilities, so its mere presence is gated.
fn host_stub(
    _plugin: &mut CurrentPlugin,
    _inputs: &[Val],
    outputs: &mut [Val],
    _user_data: UserData<()>,
) -> Result<(), ExtismError> {
    if let Some(out) = outputs.first_mut() {
        *out = Val::I64(0);
    }
    Ok(())
}

/// Build a sandboxed plugin from `module` bytes, registering host functions
/// **only** for the granted `capabilities`. WASI is off and memory / fuel /
/// timeout are bounded (see the `DEFAULT_*` consts). Returns a user-facing
/// error if the module fails to compile, link, or instantiate — including the
/// case where it imports a host function whose capability wasn't granted.
pub fn build_sandboxed_plugin(
    module: &[u8],
    capabilities: &[Capability],
) -> Result<extism::Plugin, String> {
    let manifest = Manifest::new([Wasm::data(module.to_vec())])
        .with_memory_max(DEFAULT_MEMORY_MAX_PAGES)
        .with_timeout(DEFAULT_TIMEOUT);

    let mut builder = PluginBuilder::new(manifest)
        .with_wasi(false)
        .with_fuel_limit(DEFAULT_FUEL);

    for name in host_function_names_for(capabilities) {
        builder = builder.with_function(
            name,
            [ValType::I64],
            [ValType::I64],
            UserData::default(),
            host_stub,
        );
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

    #[test]
    fn loads_a_clean_module_with_no_imports() {
        let module = wasm(
            r#"(module
                 (memory (export "memory") 1)
                 (func (export "run") (result i32) i32.const 0))"#,
        );
        assert!(build_sandboxed_plugin(&module, &[]).is_ok());
    }

    #[test]
    fn rejects_a_module_importing_an_ungranted_host_function() {
        // Imports host_foreground_window but no detect:foreground-window grant.
        let module = wasm(
            r#"(module
                 (import "extism:host/user" "host_foreground_window"
                   (func $f (param i64) (result i64)))
                 (memory (export "memory") 1)
                 (func (export "run") (result i64) (call $f (i64.const 0))))"#,
        );
        let err = build_sandboxed_plugin(&module, &[]).unwrap_err();
        assert!(
            err.contains("failed to load"),
            "ungranted import should fail the load, got: {err}"
        );
    }

    #[test]
    fn accepts_and_invokes_a_granted_host_function() {
        // `run` calls the granted host function, so building succeeds and
        // calling it actually dispatches into the registered host stub.
        let module = wasm(
            r#"(module
                 (import "extism:host/user" "host_foreground_window"
                   (func $f (param i64) (result i64)))
                 (memory (export "memory") 1)
                 (func (export "run") (result i32)
                   (drop (call $f (i64.const 0)))
                   (i32.const 0)))"#,
        );
        let mut plugin = build_sandboxed_plugin(&module, &[Capability::DetectForegroundWindow])
            .expect("granting the capability registers the host function");
        assert!(
            plugin.call::<&str, &str>("run", "").is_ok(),
            "calling the export should dispatch into the host stub and return"
        );
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
        let mut plugin = build_sandboxed_plugin(&module, &[]).expect("module loads");
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
