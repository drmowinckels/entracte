//! Shared fixtures for unit tests.
//!
//! Centralising `TempDir` creation here means a panicking test still gets
//! its scratch dir reaped by `tempfile`'s drop guard — the prior pattern
//! of `std::env::temp_dir().join(...)` plus manual `remove_dir_all` leaks
//! on failure.

pub use tempfile::TempDir;

pub fn temp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("entracte-test-")
        .tempdir()
        .expect("tempdir creation")
}
