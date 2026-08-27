#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectorPlacement {
    Docked,
    Drawer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterPlacement {
    Inline,
    OverlayTrigger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellLayout {
    pub inspector: InspectorPlacement,
    pub filter: FilterPlacement,
    pub disk_picker_trigger_visible: bool,
}

impl ShellLayout {
    pub fn for_width(width: f32) -> Self {
        Self {
            inspector: if width >= 900.0 {
                InspectorPlacement::Docked
            } else {
                InspectorPlacement::Drawer
            },
            filter: if width >= 920.0 {
                FilterPlacement::Inline
            } else {
                FilterPlacement::OverlayTrigger
            },
            disk_picker_trigger_visible: true,
        }
    }
}

pub const ABOUT_LINKS: &[(&str, &str)] = &[
    ("TELEGRAM", "https://t.me/chipda"),
    ("COMMUNITY", "https://t.me/chipdachat"),
    ("CHANNEL", "https://t.me/chipcr"),
    ("X", "https://x.com/chip1cr"),
    ("WEBSITE", "https://evgyur.pro"),
    ("HUMAN 2.0", "https://human20.app"),
    ("HUMAN 2.0 TG", "https://t.me/human20"),
    ("HYPERLIQUID RU", "https://t.me/hyperliquid_ru"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_shell_preserves_critical_access() {
        let layout = ShellLayout::for_width(760.0);
        assert_eq!(layout.inspector, InspectorPlacement::Drawer);
        assert_eq!(layout.filter, FilterPlacement::OverlayTrigger);
        assert!(layout.disk_picker_trigger_visible);
    }

    #[test]
    fn about_contains_verified_author_links() {
        assert!(ABOUT_LINKS.contains(&("TELEGRAM", "https://t.me/chipda")));
        assert!(ABOUT_LINKS.contains(&("WEBSITE", "https://evgyur.pro")));
        assert!(ABOUT_LINKS.contains(&("HUMAN 2.0 TG", "https://t.me/human20")));
        assert!(ABOUT_LINKS.contains(&("HYPERLIQUID RU", "https://t.me/hyperliquid_ru")));
    }
}
