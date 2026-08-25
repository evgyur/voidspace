use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VolumeUsage {
    pub total: u64,
    pub free: u64,
}

impl VolumeUsage {
    pub fn used(self) -> u64 {
        self.total.saturating_sub(self.free)
    }
}

#[cfg(windows)]
pub fn query(path: &Path) -> Option<VolumeUsage> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::PCWSTR};

    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut total = 0;
    let mut free = 0;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide_path.as_ptr()),
            None,
            Some(&mut total),
            Some(&mut free),
        )
        .ok()?;
    }
    Some(VolumeUsage { total, free })
}

#[cfg(not(windows))]
pub fn query(_path: &Path) -> Option<VolumeUsage> {
    None
}

pub fn format_decimal_bytes(bytes: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    const TB: f64 = 1_000_000_000_000.0;

    let bytes = bytes as f64;
    if bytes >= TB {
        format!("{:.2} TB", bytes / TB)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{VolumeUsage, format_decimal_bytes};

    #[test]
    fn used_space_saturates_when_filesystem_values_are_inconsistent() {
        assert_eq!(
            VolumeUsage {
                total: 100,
                free: 30
            }
            .used(),
            70
        );
        assert_eq!(
            VolumeUsage {
                total: 30,
                free: 100
            }
            .used(),
            0
        );
    }

    #[test]
    fn formats_observed_windows_volume_values_in_decimal_units() {
        assert_eq!(format_decimal_bytes(1_999_345_020_928), "2.00 TB");
        assert_eq!(format_decimal_bytes(1_299_193_704_448), "1.30 TB");
        assert_eq!(format_decimal_bytes(700_151_316_480), "700.2 GB");
    }
}
