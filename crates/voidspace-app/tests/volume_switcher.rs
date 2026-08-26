use std::path::Path;

use voidspace_app::{ScopePresentation, VolumeRootKey, repair_volume_focus};

#[test]
fn only_true_windows_roots_receive_keys() {
    assert_eq!(
        VolumeRootKey::from_scan_root(Path::new(r"h:\")),
        Some(VolumeRootKey::new('H').unwrap())
    );
    assert_eq!(VolumeRootKey::from_scan_root(Path::new("H:")), None);
    assert_eq!(VolumeRootKey::from_scan_root(Path::new(r"H:\books")), None);
    assert_eq!(
        VolumeRootKey::from_scan_root(Path::new(r"\\server\share")),
        None
    );
}

#[test]
fn unfocused_scope_elides_only_the_matching_absolute_drive_prefix() {
    assert_eq!(
        ScopePresentation::new(r"H:\books", false).visible_text(),
        r"\books"
    );
    assert_eq!(
        ScopePresentation::new(r"H:\books", true).visible_text(),
        r"H:\books"
    );
    assert_eq!(
        ScopePresentation::new(r"\\server\share", false).visible_text(),
        r"\\server\share"
    );
}

#[test]
fn refresh_preserves_or_repairs_focus_deterministically() {
    let roots = [
        VolumeRootKey::new('C').unwrap(),
        VolumeRootKey::new('D').unwrap(),
        VolumeRootKey::new('H').unwrap(),
    ];
    assert_eq!(
        repair_volume_focus(Some(roots[1]), 1, &roots),
        Some(roots[1])
    );
    assert_eq!(
        repair_volume_focus(Some(VolumeRootKey::new('E').unwrap()), 1, &roots),
        Some(roots[1])
    );
    assert_eq!(
        repair_volume_focus(Some(VolumeRootKey::new('Z').unwrap()), 9, &roots),
        Some(roots[2])
    );
    assert_eq!(repair_volume_focus(None, 0, &[]), None);
}
