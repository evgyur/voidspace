use std::collections::BTreeMap;
use std::path::Path;

#[derive(Default)]
pub struct VolumeDisplayRegistry {
    labels: BTreeMap<String, u16>,
    next: u16,
}

impl VolumeDisplayRegistry {
    pub fn label_for(&mut self, root: &Path) -> String {
        let key = root
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_ascii_uppercase();
        let id = *self.labels.entry(key).or_insert_with(|| {
            self.next = self.next.saturating_add(1);
            self.next
        });
        format!("VOL:{id:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_across_refresh_order() {
        let mut registry = VolumeDisplayRegistry::default();
        assert_eq!(registry.label_for(Path::new("C:\\")), "VOL:01");
        assert_eq!(registry.label_for(Path::new("F:\\")), "VOL:02");
        assert_eq!(registry.label_for(Path::new("C:\\")), "VOL:01");
    }

    #[test]
    fn normalization_is_case_insensitive_for_windows_roots() {
        let mut registry = VolumeDisplayRegistry::default();
        assert_eq!(
            registry.label_for(Path::new("c:\\")),
            registry.label_for(Path::new("C:\\"))
        );
    }
}
