const WINDOWS_CONFIRMATION_FLAGS: u32 = 0;

#[cfg(windows)]
pub(crate) fn empty_with_windows_confirmation() -> Result<(), String> {
    use windows::{Win32::UI::Shell::SHEmptyRecycleBinW, core::PCWSTR};

    // Zero flags deliberately preserve the native confirmation, progress UI, and sound.
    unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), WINDOWS_CONFIRMATION_FLAGS) }
        .map_err(|error| format!("Cannot empty Recycle Bin: {error}"))
}

#[cfg(not(windows))]
pub(crate) fn empty_with_windows_confirmation() -> Result<(), String> {
    Err("Empty Recycle Bin is available only on Windows".to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn global_recycle_bin_cleanup_keeps_the_native_windows_confirmation() {
        assert_eq!(super::WINDOWS_CONFIRMATION_FLAGS, 0);
    }
}
