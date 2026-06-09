//! Local-only plugin API (#156): the parse / validate / signature-verify
//! core for plugin manifests. This is slice 1 of the staged plan in
//! `docs/developer/plugin-api-design.md` — pure functions only, no runtime,
//! no bundle I/O, no IPC. Those land in later slices; keeping this layer
//! pure mirrors how `scheduler::content_pack` landed before its commands.
//!
//! A plugin is a signed bundle whose root is a `manifest.json`. The manifest
//! declares one `kind` (content / detector / export). Code-bearing kinds
//! reference a wasm `module` and list the host-function capabilities it
//! `imports`; each import is a permission request the user must grant. The
//! signature binds the manifest **and** the module's hash, so a tampered
//! module fails verification even if the manifest is untouched.

// This is the foundational slice: the parse/validate/verify core has no
// in-crate callers yet (the install/consent commands and the wasm runtime
// that consume it land in later slices per the design doc's staging plan),
// so its items read as dead in the non-test build. The allow is scoped to
// this module and removed as soon as the next slice wires in a caller.
#![allow(dead_code)]

mod manifest;
mod signature;

// The module's public surface, re-exported so this slice defines the API in
// one place even before anything consumes it.
#[allow(unused_imports)]
pub use manifest::{
    parse_manifest, validate_manifest, Capability, Manifest, PluginKind, Signature,
    MANIFEST_VERSION, SUPPORTED_ABI_VERSION,
};
#[allow(unused_imports)]
pub use signature::{sha256, verify_signature};
