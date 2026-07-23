fn main() {
    // The CoreAudio symbols that `src/media.rs` (`audio_tap`) calls are newer
    // than the app's macOS floor:
    //   AudioHardwareCreate/DestroyProcessTap      — macOS 14.2
    //   AudioHardwareCreate/DestroyAggregateDevice — macOS 13.0
    // objc2-core-audio declares them in a plain `extern` block with no
    // availability, so the linker strong-binds them and the app would fail to
    // LOAD (`dyld: Symbol not found`) on older macOS — before any runtime check
    // can run. Mark them as weak references so they resolve to null on systems
    // that lack them; `audio_tap::output_active` gates every call behind
    // `isOperatingSystemAtLeastVersion(14.2)`, so a null symbol is never
    // dereferenced. (`CATapDescription` needs no flag — objc2 resolves the
    // class via a runtime `objc_getClass` lookup, not a load-time symbol.)
    // Weak-link the whole CoreAudio framework: every symbol becomes a weak
    // reference, so the four newer functions resolve to null on older macOS
    // instead of aborting at load, while the long-standing CoreAudio symbols
    // (always present on our floor) still bind normally. Per-symbol `-Wl,-U`
    // does not work here — modern ld64 leaves link-time-resolvable symbols
    // strong-bound — so the framework-level weak link is the mechanism.
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg-bins=-Wl,-weak_framework,CoreAudio");

    tauri_build::build()
}
