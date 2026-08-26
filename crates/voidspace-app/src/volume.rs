use std::path::{Path, PathBuf};

const FALLBACK_LABEL: &str = "Local Disk";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VolumeInfo {
    pub root_path: PathBuf,
    pub display_root: String,
    pub label: String,
    pub usage: VolumeUsage,
}

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

pub fn used_ratio(usage: VolumeUsage) -> f32 {
    if usage.total == 0 {
        0.0
    } else {
        (usage.used() as f64 / usage.total as f64).clamp(0.0, 1.0) as f32
    }
}

pub fn used_percentage(usage: VolumeUsage) -> u8 {
    (used_ratio(usage) * 100.0).round() as u8
}

pub fn display_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        FALLBACK_LABEL.to_owned()
    } else {
        label.to_owned()
    }
}

fn drive_root_strings(mask: u32) -> Vec<String> {
    (0..26)
        .filter(|index| mask & (1 << index) != 0)
        .map(|index| format!("{}:\\", (b'A' + index as u8) as char))
        .collect()
}

#[cfg(windows)]
pub fn list() -> Result<Vec<VolumeInfo>, String> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;

    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }

    let volumes = drive_root_strings(mask)
        .into_iter()
        .filter_map(|root| {
            let root_path = PathBuf::from(&root);
            let usage = query(&root_path)?;
            let label = query_label(&root_path)
                .map(|label| display_label(&label))
                .unwrap_or_else(|| FALLBACK_LABEL.to_owned());
            Some(VolumeInfo {
                display_root: root.trim_end_matches('\\').to_owned(),
                root_path,
                label,
                usage,
            })
        })
        .collect();
    Ok(volumes)
}

#[cfg(not(windows))]
pub fn list() -> Result<Vec<VolumeInfo>, String> {
    Ok(Vec::new())
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

#[cfg(windows)]
fn query_label(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{Win32::Storage::FileSystem::GetVolumeInformationW, core::PCWSTR};

    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut label = [0_u16; 261];
    unsafe {
        GetVolumeInformationW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            None,
        )
        .ok()?;
    }
    let length = label.iter().position(|character| *character == 0)?;
    Some(String::from_utf16_lossy(&label[..length]))
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
    use super::{
        VolumeUsage, display_label, drive_root_strings, format_decimal_bytes, used_percentage,
        used_ratio,
    };

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

    #[test]
    fn logical_drive_mask_becomes_ordered_roots() {
        let mask = (1 << 25) | (1 << 2) | (1 << 3);
        assert_eq!(drive_root_strings(mask), ["C:\\", "D:\\", "Z:\\"]);
    }

    #[test]
    fn volume_label_falls_back_only_when_blank() {
        assert_eq!(display_label(""), "Local Disk");
        assert_eq!(display_label("   "), "Local Disk");
        assert_eq!(display_label("  Windows  "), "Windows");
    }

    #[test]
    fn used_percentage_is_clamped_and_zero_capacity_is_safe() {
        let empty = VolumeUsage { total: 0, free: 0 };
        let normal = VolumeUsage {
            total: 1_000,
            free: 349,
        };
        let inconsistent = VolumeUsage {
            total: 100,
            free: 200,
        };
        assert_eq!(used_ratio(empty), 0.0);
        assert_eq!(used_percentage(empty), 0);
        assert_eq!(used_percentage(normal), 65);
        assert_eq!(used_percentage(inconsistent), 0);
    }
}
