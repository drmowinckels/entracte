//! Local-only plugin API (#156): manifests, signatures, the installed-plugin
//! registry, and the install/uninstall orchestration. See the staged plan in
//! `docs/developer/plugin-api-design.md`.
//!
//! A plugin is a signed bundle whose root is a `manifest.json`. The manifest
//! declares one `kind` (content / detector / export). Code-bearing kinds
//! reference a wasm `module` and list the host-function capabilities they
//! `import`; each import is a permission request the user must grant. The
//! signature binds the manifest **and** the module's hash, so a tampered
//! module fails verification even if the manifest is untouched.
//!
//! This slice ships **content providers** end to end: a content plugin
//! carries a typed content pack, merged into the active profile on install
//! and removed exactly on uninstall (merge-and-track). Detector and export
//! plugins parse and validate here but cannot yet be installed — they need
//! the wasm runtime (a later slice).

mod install;
mod manifest;
pub(crate) mod registry;
mod signature;

pub use install::prepare_content_install;
pub use registry::{InstalledPlugin, PluginRegistry, PluginSummary};

// The full manifest/signature API surface. Some items are consumed by the
// command layer and tests now; others (the ABI version, module hashing, the
// raw parse/validate/verify entry points) by the wasm-runtime slice. Marked
// allow(unused_imports) so the public API can live in one place ahead of all
// its consumers.
#[allow(unused_imports)]
pub use manifest::{
    parse_manifest, validate_manifest, Capability, Manifest, PluginKind, Signature,
    MANIFEST_VERSION, SUPPORTED_ABI_VERSION,
};
#[allow(unused_imports)]
pub use signature::{sha256, verify_signature};
