pub fn is_active() -> bool {
    #[cfg(target_os = "macos")]
    return macos::check();
    #[cfg(target_os = "windows")]
    return windows::check();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return false;
}

#[cfg(target_os = "macos")]
mod macos {
    pub fn check() -> bool {
        let Some(home) = std::env::var_os("HOME") else {
            return false;
        };
        let path = std::path::Path::new(&home).join("Library/DoNotDisturb/DB/Assertions.json");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return false;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };
        parsed
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter().any(|entry| {
                    entry
                        .get("storeAssertionRecords")
                        .and_then(|r| r.as_array())
                        .map(|records| !records.is_empty())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::sync::OnceLock;
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    const WNF_FOCUS_ASSIST: u64 = 0xA3BC1875_A3BC0875;
    // Windows 10 1809 (October 2018) was the first build where Focus
    // Assist exposed its state through this WNF name with the
    // six-argument NtQueryWnfStateData signature we transmute below.
    const MIN_SUPPORTED_BUILD: u32 = 17763;

    type NtQueryWnfStateDataFn = unsafe extern "system" fn(
        *const u64,
        *const u8,
        *const u8,
        *mut u32,
        *mut u8,
        *mut u32,
    ) -> i32;

    fn os_build() -> Option<u32> {
        let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        let status = unsafe { RtlGetVersion(&mut info) };
        if status != 0 {
            return None;
        }
        Some(info.dwBuildNumber)
    }

    fn version_supported() -> bool {
        static CACHED: OnceLock<bool> = OnceLock::new();
        *CACHED.get_or_init(|| match os_build() {
            Some(build) if build >= MIN_SUPPORTED_BUILD => true,
            Some(build) => {
                log::info!(
                    "dnd: Windows build {build} < {MIN_SUPPORTED_BUILD}; \
                     skipping Focus Assist probe"
                );
                false
            }
            None => {
                log::info!("dnd: RtlGetVersion failed; skipping Focus Assist probe");
                false
            }
        })
    }

    // SAFETY: The signature for `NtQueryWnfStateData` is undocumented but
    // has been stable across Windows 10 build 17763+ and all Windows 11
    // builds shipped to date. `version_supported` gates the transmute to
    // those releases. On older builds, or if `RtlGetVersion` fails, we
    // return `false` from `check()` without ever calling the symbol.
    fn query_fn() -> Option<NtQueryWnfStateDataFn> {
        static CACHED: OnceLock<Option<NtQueryWnfStateDataFn>> = OnceLock::new();
        *CACHED.get_or_init(|| unsafe {
            if !version_supported() {
                return None;
            }
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
            if ntdll.is_null() {
                return None;
            }
            let ptr = GetProcAddress(ntdll, c"NtQueryWnfStateData".as_ptr().cast());
            ptr.map(|p| std::mem::transmute::<_, NtQueryWnfStateDataFn>(p))
        })
    }

    pub fn check() -> bool {
        let Some(query) = query_fn() else {
            return false;
        };
        let state_name = WNF_FOCUS_ASSIST;
        let mut buffer = [0u8; 4];
        let mut buffer_size: u32 = buffer.len() as u32;
        let mut change_stamp: u32 = 0;
        let status = unsafe {
            query(
                &state_name,
                std::ptr::null(),
                std::ptr::null(),
                &mut change_stamp,
                buffer.as_mut_ptr(),
                &mut buffer_size,
            )
        };
        if status != 0 || buffer_size < 4 {
            return false;
        }
        let mode = u32::from_le_bytes(buffer);
        mode > 0
    }
}
