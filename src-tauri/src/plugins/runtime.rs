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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use extism::{
    CurrentPlugin, Error as ExtismError, Manifest, PluginBuilder, UserData, Val, ValType, Wasm,
};

use super::detect;
use super::manifest::Capability;

/// The host-function name a detector calls to vote "suppress the next break".
/// Ungated (every detector may report a verdict) and always registered, so a
/// detector module always links against it.
const HOST_SUPPRESS: &str = "host_suppress";

/// Per-install context the host functions need beyond the capability scopes
/// themselves, plus the verdict channel.
#[derive(Debug, Clone, Default)]
pub struct SandboxContext {
    /// The process pattern a `detect:processes` grant matches against (from
    /// the manifest's detect config). The `detect:file:<path>` scope lives in
    /// the capability itself.
    pub process_pattern: Option<String>,
    /// Set `true` when the module calls `host_suppress()` during a `detect()`
    /// run. The evaluator resets it before each run and reads it after, so a
    /// cached plugin's verdict is fresh each cycle.
    pub verdict: Arc<AtomicBool>,
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

/// Register a `() -> i64` boolean host function whose answer is `probe(data)`.
/// `data` is captured host-side from the grant/context — never supplied by
/// the module — so the probe is scope-checked by construction.
fn register_probe<'a, T, F>(
    builder: PluginBuilder<'a>,
    name: &str,
    data: T,
    probe: F,
) -> PluginBuilder<'a>
where
    T: Send + Sync + 'static,
    F: Fn(&T) -> bool + Send + Sync + 'static,
{
    builder.with_function(
        name,
        [],
        [ValType::I64],
        UserData::new(data),
        move |_p, _in, out, ud| {
            let guard = ud.get()?;
            set_bool(out, probe(&guard.lock().unwrap()));
            Ok(())
        },
    )
}

/// Register the host function a single capability unlocks. All host functions
/// are `() -> i64` booleans in this ABI; the data each one reads (the process
/// pattern, the granted file path) is captured from the grant + context, so a
/// module cannot influence what's probed.
fn register_capability<'a>(
    builder: PluginBuilder<'a>,
    cap: &Capability,
    ctx: &SandboxContext,
) -> PluginBuilder<'a> {
    let name = host_function_name(cap);
    match cap {
        Capability::DetectProcesses => {
            let pattern = ctx.process_pattern.clone().unwrap_or_default();
            register_probe(builder, name, pattern, |p: &String| {
                detect::process_running(p)
            })
        }
        Capability::DetectFile(path) => {
            register_probe(builder, name, PathBuf::from(path), |p: &PathBuf| {
                detect::read_flag(p)
            })
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

    // The ungated verdict channel: calling it flips the shared flag.
    let verdict = ctx.verdict.clone();
    builder = builder.with_function(
        HOST_SUPPRESS,
        [],
        [],
        UserData::new(verdict),
        |_p, _in, _out, ud| {
            ud.get()?.lock().unwrap().store(true, Ordering::Relaxed);
            Ok(())
        },
    );

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

/// Build a detector from its module + granted capabilities, run its `detect()`
/// export once, and return whether it voted to suppress (by calling
/// `host_suppress`). A module that fails to build, has no `detect` export, or
/// traps is treated as **no suppression** — a broken detector never blocks a
/// break. Pure aside from the probes the granted host functions perform.
pub fn evaluate_detector(
    module: &[u8],
    capabilities: &[Capability],
    process_pattern: Option<String>,
) -> bool {
    let ctx = SandboxContext {
        process_pattern,
        verdict: Arc::new(AtomicBool::new(false)),
    };
    let verdict = ctx.verdict.clone();
    let mut plugin = match build_sandboxed_plugin(module, capabilities, &ctx) {
        Ok(plugin) => plugin,
        Err(_) => return false,
    };
    verdict.store(false, Ordering::Relaxed);
    // We don't care about the call's output/return — only whether the module
    // reported a verdict before returning or trapping.
    let _ = plugin.call::<&str, &str>("detect", "");
    verdict.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).expect("valid WAT")
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
        // "entracte" is a whole token of the test binary's process name on
        // every platform (see the detect.rs test for the rationale).
        // Pattern matches a live process → host returns true → the module
        // traps → the call errors.
        let module = trap_if_true_module("host_process_running");
        let mut plugin = build_sandboxed_plugin(
            &module,
            &[Capability::DetectProcesses],
            &SandboxContext {
                process_pattern: Some("entracte".to_string()),
                ..Default::default()
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
                ..Default::default()
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

    /// A detector module that votes to suppress (via `host_suppress`) when
    /// the granted `host_process_running` probe matches.
    fn suppress_if_process_module() -> Vec<u8> {
        wasm(
            r#"(module
                 (import "extism:host/user" "host_process_running" (func $proc (result i64)))
                 (import "extism:host/user" "host_suppress" (func $suppress))
                 (memory (export "memory") 1)
                 (func (export "detect") (result i32)
                   (if (i64.ne (call $proc) (i64.const 0)) (then (call $suppress)))
                   (i32.const 0)))"#,
        )
    }

    #[test]
    fn evaluate_detector_reports_the_modules_verdict() {
        let module = suppress_if_process_module();
        // "entracte" matches the live test binary → module votes suppress.
        assert!(evaluate_detector(
            &module,
            &[Capability::DetectProcesses],
            Some("entracte".to_string())
        ));
        // A pattern matching nothing → no vote → no suppression.
        assert!(!evaluate_detector(
            &module,
            &[Capability::DetectProcesses],
            Some("entracte-no-such-process-zzz".to_string())
        ));
    }

    #[test]
    fn evaluate_detector_is_false_when_the_module_does_not_vote() {
        // `detect` exists but calls nothing — no verdict reported.
        let module = wasm(
            r#"(module
                 (memory (export "memory") 1)
                 (func (export "detect") (result i32) (i32.const 0)))"#,
        );
        assert!(!evaluate_detector(&module, &[], None));
    }

    #[test]
    fn evaluate_detector_treats_a_broken_module_as_no_suppression() {
        // No `detect` export → the call fails → false, not a blocked break.
        let no_export = wasm(r#"(module (memory (export "memory") 1))"#);
        assert!(!evaluate_detector(&no_export, &[], None));

        // Imports an ungranted host function → fails to build → false.
        let ungranted = trap_if_true_module("host_foreground_window");
        assert!(!evaluate_detector(&ungranted, &[], None));
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
