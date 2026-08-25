use proptest::prelude::*;
use voidspace_layout::{Rect, layout_weights};

fn overlap_area(a: Rect, b: Rect) -> f32 {
    let width = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0.0);
    let height = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0.0);
    width * height
}

proptest! {
    #[test]
    fn children_are_contained_and_non_overlapping(
        weights in prop::collection::vec(1u64..1_000_000, 1..200)
    ) {
        let bounds = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let out = layout_weights(&weights, bounds);
        prop_assert_eq!(out.len(), weights.len());
        for rect in &out {
            prop_assert!(bounds.contains(*rect));
            prop_assert!(rect.width() >= 0.0 && rect.height() >= 0.0);
        }
        for i in 0..out.len() {
            for j in (i + 1)..out.len() {
                prop_assert!(overlap_area(out[i], out[j]) <= 0.5);
            }
        }
        let covered: f32 = out.iter().map(|rect| rect.area()).sum();
        prop_assert!((covered - bounds.area()).abs() <= bounds.area() * 0.001 + 1.0);
    }
}

#[test]
fn layout_is_deterministic() {
    let weights = [5, 4, 3, 2, 1];
    let bounds = Rect::new(0.0, 0.0, 100.0, 70.0);
    assert_eq!(
        layout_weights(&weights, bounds),
        layout_weights(&weights, bounds)
    );
}

#[test]
fn wide_canvas_places_the_dominant_tile_on_the_left() {
    let bounds = Rect::new(0.0, 0.0, 1000.0, 500.0);
    let out = layout_weights(&[55, 20, 15, 10], bounds);
    let dominant = out[0];

    assert_eq!(dominant.min_x, bounds.min_x);
    assert_eq!(dominant.min_y, bounds.min_y);
    assert_eq!(dominant.max_y, bounds.max_y);
    assert!(dominant.width() > 400.0);
    assert!(dominant.width() < 700.0);
}
