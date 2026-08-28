use std::{f32::consts::PI, path::PathBuf};

use egui::{Align2, Color32, FontId, Id, Key, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use voidspace_model::{FileIdentity, NodeId, NodeKind, ScanId};

use crate::hud;

const INNER_RADIUS: f32 = 42.0;
const OUTER_RADIUS: f32 = 106.0;
const HUB_RADIUS: f32 = 30.0;
const SECTOR_HALF_ANGLE: f32 = 18.0_f32.to_radians();
const RAIL_WIDTH: f32 = 152.0;
const RAIL_HEIGHT: f32 = 44.0;
const SAFE_MARGIN: f32 = 8.0;

mod primitives {
    use egui::{Color32, Stroke};
    use unicode_segmentation::UnicodeSegmentation;

    // Remove each staging allowance as the target-plate and hover slices consume the item.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) const REACTOR_BEAT_SECONDS: f32 = 1.35;
    pub(super) const ACTIVE_CYAN: Color32 = Color32::from_rgb(0x1e, 0xcd, 0xe2);
    pub(super) const ACTIVE_LIME: Color32 = Color32::from_rgb(0xbd, 0xff, 0x3e);
    pub(super) const ACTIVE_MAGENTA: Color32 = Color32::from_rgb(0xff, 0x49, 0xbc);
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) const ACTIVE_LABEL_INK: Color32 = Color32::from_rgb(0x07, 0x09, 0x0a);
    #[cfg_attr(not(test), allow(dead_code))]
    const ELLIPSIS: &str = "…";

    #[cfg_attr(not(test), allow(dead_code))]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) struct ActiveSectorStyle {
        pub(super) fill: Color32,
        pub(super) inner_stroke: Stroke,
        pub(super) outer_bloom: Stroke,
        pub(super) label_color: Color32,
        pub(super) label_scale: f32,
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn reactor_beat(elapsed_seconds: f32) -> f32 {
        const KEYFRAMES: [(f32, f32); 6] = [
            (0.0, 0.0),
            (0.12, 1.0),
            (0.22, 0.0),
            (0.34, 1.0),
            (0.45, 0.0),
            (1.0, 0.0),
        ];

        let phase = (elapsed_seconds / REACTOR_BEAT_SECONDS).rem_euclid(1.0);
        for keyframes in KEYFRAMES.windows(2) {
            let (start_phase, start_intensity) = keyframes[0];
            let (end_phase, end_intensity) = keyframes[1];
            if phase <= end_phase {
                let position = (phase - start_phase) / (end_phase - start_phase);
                let smoothstep = position * position * (3.0 - 2.0 * position);
                return start_intensity + (end_intensity - start_intensity) * smoothstep;
            }
        }
        0.0
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn active_sector_style(
        semantic_color: Color32,
        intensity: f32,
        geometry_scale: f32,
    ) -> ActiveSectorStyle {
        let intensity = intensity.clamp(0.0, 1.0);
        let geometry_scale = geometry_scale.max(0.0);
        let fill = Color32::from_rgb(semantic_color.r(), semantic_color.g(), semantic_color.b());
        let bloom_alpha = (72.0 + 112.0 * intensity).round() as u8;
        let bloom_color =
            Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), bloom_alpha);
        ActiveSectorStyle {
            fill,
            inner_stroke: Stroke::new(2.0 * geometry_scale, fill),
            outer_bloom: Stroke::new((2.0 + 3.0 * intensity) * geometry_scale, bloom_color),
            label_color: ACTIVE_LABEL_INK,
            label_scale: 1.0 + 0.095 * intensity,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn end_ellipsis<F>(
        value: &str,
        maximum_width: f32,
        minimum_leading_graphemes: usize,
        measure: F,
    ) -> Option<String>
    where
        F: Fn(&str) -> f32,
    {
        if measure(value) <= maximum_width {
            return Some(value.to_owned());
        }

        let graphemes = value.graphemes(true).collect::<Vec<_>>();
        if minimum_leading_graphemes >= graphemes.len() {
            return None;
        }
        for leading_count in (minimum_leading_graphemes..graphemes.len()).rev() {
            let mut candidate = graphemes[..leading_count].concat();
            candidate.push_str(ELLIPSIS);
            if measure(&candidate) <= maximum_width {
                return Some(candidate);
            }
        }
        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn middle_ellipsis<F>(
        value: &str,
        preserved_prefix: &str,
        preserved_suffix: &str,
        maximum_width: f32,
        measure: F,
    ) -> Option<String>
    where
        F: Fn(&str) -> f32,
    {
        let graphemes = value.graphemes(true).collect::<Vec<_>>();
        let prefix = preserved_prefix.graphemes(true).collect::<Vec<_>>();
        let suffix = preserved_suffix.graphemes(true).collect::<Vec<_>>();
        if prefix.len() + suffix.len() > graphemes.len()
            || !graphemes.starts_with(&prefix)
            || !graphemes.ends_with(&suffix)
        {
            return None;
        }
        if measure(value) <= maximum_width {
            return Some(value.to_owned());
        }

        let optional_count = graphemes.len() - prefix.len() - suffix.len();
        if optional_count == 0 {
            return None;
        }

        let candidate_for = |retained_count: usize| {
            let left_count = retained_count / 2;
            let right_count = retained_count - left_count;
            let left_end = prefix.len() + left_count;
            let right_start = graphemes.len() - suffix.len() - right_count;
            let mut candidate = graphemes[..left_end].concat();
            candidate.push_str(ELLIPSIS);
            candidate.extend(graphemes[right_start..].iter().copied());
            candidate
        };

        let mut best = candidate_for(0);
        if measure(&best) > maximum_width {
            return None;
        }

        let mut low = 1;
        let mut high = optional_count - 1;
        while low <= high {
            let retained_count = low + (high - low) / 2;
            let candidate = candidate_for(retained_count);
            if measure(&candidate) <= maximum_width {
                best = candidate;
                low = retained_count + 1;
            } else {
                high = retained_count - 1;
            }
        }
        Some(best)
    }
}

use primitives::{ACTIVE_CYAN, ACTIVE_LIME, ACTIVE_MAGENTA};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextTarget {
    pub scan_id: ScanId,
    pub generation: u64,
    pub node_id: NodeId,
    pub identity: FileIdentity,
    pub path: PathBuf,
    pub kind: NodeKind,
    pub root: PathBuf,
    pub view_root: NodeId,
    pub display_name: String,
    pub display_size: String,
    pub origin_focus: Id,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TacticalAction {
    OpenInExplorer,
    Recycle,
    DeletePermanently,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TacticalArcOutcome {
    Action(TacticalAction),
    Dismiss,
}

#[derive(Clone, Copy)]
struct SectorSpec {
    action: TacticalAction,
    short: &'static str,
    label: &'static str,
    accessible_name: &'static str,
    shortcut: &'static str,
    color: Color32,
}

const SECTORS: [SectorSpec; 3] = [
    SectorSpec {
        action: TacticalAction::OpenInExplorer,
        short: "OPEN",
        label: "OPEN IN EXPLORER",
        accessible_name: "Open in Explorer",
        shortcut: "1",
        color: ACTIVE_CYAN,
    },
    SectorSpec {
        action: TacticalAction::Recycle,
        short: "BIN",
        label: "MOVE TO RECYCLE BIN",
        accessible_name: "Move to Recycle Bin",
        shortcut: "2",
        color: ACTIVE_LIME,
    },
    SectorSpec {
        action: TacticalAction::DeletePermanently,
        short: "VOID",
        label: "DELETE WITHOUT RECOVERY",
        accessible_name: "Delete without recovery",
        shortcut: "3",
        color: ACTIVE_MAGENTA,
    },
];

impl TacticalAction {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::OpenInExplorer, Self::Recycle, Self::DeletePermanently];

    pub const fn label(self) -> &'static str {
        SECTORS[self.index()].label
    }

    pub const fn accessible_name(self) -> &'static str {
        SECTORS[self.index()].accessible_name
    }

    pub const fn keyboard_hint(self) -> &'static str {
        SECTORS[self.index()].shortcut
    }

    const fn index(self) -> usize {
        match self {
            Self::OpenInExplorer => 0,
            Self::Recycle => 1,
            Self::DeletePermanently => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Orientation {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug)]
pub struct TacticalArcGeometry {
    pub center: Pos2,
    origin: Pos2,
    orientation: Orientation,
    scale: f32,
    work_area: Rect,
}

impl TacticalArcGeometry {
    #[cfg(test)]
    pub fn new(center: Pos2, inner_radius: f32, outer_radius: f32) -> Self {
        let scale = outer_radius / OUTER_RADIUS;
        debug_assert!((inner_radius - INNER_RADIUS * scale).abs() < 1.0);
        Self {
            center,
            origin: center,
            orientation: Orientation::Right,
            scale,
            work_area: Rect::from_center_size(center, Vec2::splat(outer_radius * 4.0)),
        }
    }

    #[cfg(test)]
    pub fn clamped(requested: Pos2, area: Rect, outer_radius: f32) -> Self {
        let scale = outer_radius / OUTER_RADIUS;
        Self::fit(requested, area, scale).expect("test area fits tactical arc")
    }

    fn fit(requested: Pos2, area: Rect, preferred_scale: f32) -> Option<Self> {
        let full_width =
            (OUTER_RADIUS * 2.0 + RAIL_WIDTH + 8.0) * preferred_scale + SAFE_MARGIN * 2.0;
        let full_height = OUTER_RADIUS * 2.0 * preferred_scale + SAFE_MARGIN * 2.0;
        let scale = if area.width() >= full_width && area.height() >= full_height {
            preferred_scale
        } else {
            preferred_scale * 0.75
        };
        let minimum_width = (OUTER_RADIUS * 2.0 + RAIL_WIDTH + 8.0) * scale + SAFE_MARGIN * 2.0;
        let minimum_height = OUTER_RADIUS * 2.0 * scale + SAFE_MARGIN * 2.0;
        if area.width() < minimum_width || area.height() < minimum_height {
            return None;
        }

        let right_space = area.right() - requested.x;
        let left_space = requested.x - area.left();
        let orientation = if right_space >= left_space {
            Orientation::Right
        } else {
            Orientation::Left
        };
        let fan = OUTER_RADIUS * scale;
        let rail = (OUTER_RADIUS + RAIL_WIDTH + 8.0) * scale;
        let (left_extent, right_extent) = match orientation {
            Orientation::Right => (rail, fan),
            Orientation::Left => (fan, rail),
        };
        let center = Pos2::new(
            requested.x.clamp(
                area.left() + left_extent + SAFE_MARGIN,
                area.right() - right_extent - SAFE_MARGIN,
            ),
            requested.y.clamp(
                area.top() + fan + SAFE_MARGIN,
                area.bottom() - fan - SAFE_MARGIN,
            ),
        );
        Some(Self {
            center,
            origin: requested,
            orientation,
            scale,
            work_area: area,
        })
    }

    pub fn bounds(self) -> Rect {
        let fan = OUTER_RADIUS * self.scale;
        let circle = Rect::from_center_size(self.center, Vec2::splat(fan * 2.0));
        circle.union(self.rail_rect())
    }

    fn rail_rect(self) -> Rect {
        let size = Vec2::new(RAIL_WIDTH, RAIL_HEIGHT) * self.scale;
        let x = match self.orientation {
            Orientation::Right => {
                self.center.x - OUTER_RADIUS * self.scale - size.x - 8.0 * self.scale
            }
            Orientation::Left => self.center.x + OUTER_RADIUS * self.scale + 8.0 * self.scale,
        };
        Rect::from_min_size(Pos2::new(x, self.center.y - size.y * 0.5), size)
    }

    fn action_angle(self, index: usize) -> f32 {
        let right_angle = [-44.0_f32, 0.0, 44.0][index].to_radians();
        match self.orientation {
            Orientation::Right => right_angle,
            Orientation::Left => PI - right_angle,
        }
    }

    fn action_center(self, index: usize) -> Pos2 {
        let radius = (INNER_RADIUS + OUTER_RADIUS) * 0.5 * self.scale;
        self.center + Vec2::angled(self.action_angle(index)) * radius
    }

    pub fn hit_test(self, pointer: Pos2) -> Option<TacticalAction> {
        let delta = pointer - self.center;
        let radius = delta.length();
        if radius < INNER_RADIUS * self.scale || radius > OUTER_RADIUS * self.scale {
            return None;
        }
        let angle = delta.angle();
        SECTORS.iter().enumerate().find_map(|(index, sector)| {
            let difference = angle_difference(angle, self.action_angle(index)).abs();
            (difference <= SECTOR_HALF_ANGLE).then_some(sector.action)
        })
    }
}

fn angle_difference(a: f32, b: f32) -> f32 {
    (a - b + PI).rem_euclid(PI * 2.0) - PI
}

fn geometry_for_area_painter(
    geometry: TacticalArcGeometry,
    painter_clip: Rect,
) -> TacticalArcGeometry {
    debug_assert!(geometry.work_area.contains(geometry.bounds().min));
    debug_assert!(geometry.work_area.contains(geometry.bounds().max));
    debug_assert!(painter_clip.intersects(geometry.bounds()));
    geometry
}

#[derive(Clone, Debug)]
pub struct TacticalArcState {
    pub target: ContextTarget,
    pub geometry: TacticalArcGeometry,
    keyboard_index: Option<usize>,
    armed: bool,
}

impl TacticalArcState {
    pub fn new(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
    ) -> Option<Self> {
        Some(Self {
            target,
            geometry: TacticalArcGeometry::fit(pointer, work_area, 1.0)?,
            keyboard_index: keyboard_open.then_some(0),
            armed: false,
        })
    }

    pub fn show(&mut self, context: &egui::Context, font: FontId) -> Option<TacticalArcOutcome> {
        let mut chosen = None;
        let ignore_opening_secondary_click = !self.armed;
        self.armed = true;
        let geometry = self.geometry;
        let area_rect = geometry.bounds().expand(SAFE_MARGIN);
        if geometry.origin.distance(geometry.center) > 2.0 {
            context
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    Id::new("tactical-tether"),
                ))
                .line_segment(
                    [geometry.origin, geometry.center],
                    Stroke::new(1.0, hud::ORANGE),
                );
        }
        let response = egui::Area::new(Id::new("tactical-arc"))
            .order(egui::Order::Foreground)
            .fixed_pos(area_rect.min)
            .show(context, |ui| {
                ui.set_min_size(area_rect.size());
                let (rect, response) = ui.allocate_exact_size(area_rect.size(), Sense::click());
                let painter = ui.painter_at(rect);
                let draw_geometry = geometry_for_area_painter(geometry, rect);
                let center = draw_geometry.center;
                let pointer = ui.ctx().pointer_hover_pos();
                let hovered = pointer.and_then(|position| geometry.hit_test(position));

                for (index, sector) in SECTORS.iter().enumerate() {
                    let active = hovered == Some(sector.action)
                        || (hovered.is_none() && self.keyboard_index == Some(index));
                    let angle = draw_geometry.action_angle(index);
                    let mut outer_points = Vec::with_capacity(13);
                    let mut inner_points = Vec::with_capacity(13);
                    for step in 0..=12 {
                        let fraction = step as f32 / 12.0;
                        let sample = angle - SECTOR_HALF_ANGLE + SECTOR_HALF_ANGLE * 2.0 * fraction;
                        outer_points.push(
                            center + Vec2::angled(sample) * (OUTER_RADIUS * draw_geometry.scale),
                        );
                        inner_points.push(
                            center + Vec2::angled(sample) * (INNER_RADIUS * draw_geometry.scale),
                        );
                    }
                    let fill = if active {
                        Color32::from_rgba_unmultiplied(
                            sector.color.r(),
                            sector.color.g(),
                            sector.color.b(),
                            44,
                        )
                    } else {
                        Color32::from_rgba_unmultiplied(11, 14, 16, 245)
                    };
                    let mut mesh = egui::Mesh::default();
                    for step in 0..12 {
                        let base = mesh.vertices.len() as u32;
                        for point in [
                            inner_points[step],
                            outer_points[step],
                            outer_points[step + 1],
                            inner_points[step + 1],
                        ] {
                            mesh.colored_vertex(point, fill);
                        }
                        mesh.add_triangle(base, base + 1, base + 2);
                        mesh.add_triangle(base, base + 2, base + 3);
                    }
                    painter.add(Shape::mesh(mesh));
                    let sector_stroke = Stroke::new(if active { 2.0 } else { 1.0 }, sector.color);
                    painter.add(Shape::line(outer_points.clone(), sector_stroke));
                    painter.add(Shape::line(inner_points.clone(), sector_stroke));
                    painter.line_segment([inner_points[0], outer_points[0]], sector_stroke);
                    painter.line_segment([inner_points[12], outer_points[12]], sector_stroke);
                    let action_center = draw_geometry.action_center(index);
                    painter.text(
                        action_center,
                        Align2::CENTER_CENTER,
                        format!("{}  {}", sector.shortcut, sector.short),
                        font.clone(),
                        sector.color,
                    );
                    let action_response = ui.interact(
                        Rect::from_center_size(
                            action_center,
                            Vec2::splat(56.0 * draw_geometry.scale),
                        ),
                        Id::new("tactical-action").with(index),
                        Sense::click(),
                    );
                    action_response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!(
                                "{} · key {} to select · Enter to execute",
                                sector.action.accessible_name(),
                                sector.action.keyboard_hint()
                            ),
                        )
                    });
                }

                painter.circle_filled(
                    center,
                    HUB_RADIUS * draw_geometry.scale,
                    Color32::from_rgb(7, 9, 10),
                );
                painter.circle_stroke(
                    center,
                    HUB_RADIUS * draw_geometry.scale,
                    Stroke::new(1.0, hud::ORANGE),
                );
                painter.text(
                    center + Vec2::new(0.0, -4.0),
                    Align2::CENTER_BOTTOM,
                    &self.target.display_size,
                    font.clone(),
                    hud::ORANGE,
                );
                painter.text(
                    center + Vec2::new(0.0, 4.0),
                    Align2::CENTER_TOP,
                    truncate(&self.target.display_name, 13),
                    font.clone(),
                    Color32::WHITE,
                );

                if let Some(pointer) = pointer {
                    painter.line_segment([center, pointer], Stroke::new(1.5, hud::CYAN));
                }
                let rail = draw_geometry.rail_rect();
                painter.rect_filled(rail, 0.0, hud::PANEL_RAISED);
                painter.rect_stroke(
                    rail,
                    0.0,
                    Stroke::new(1.0, hud::HAIRLINE),
                    egui::StrokeKind::Inside,
                );
                let selected = hovered
                    .map(TacticalAction::index)
                    .or(self.keyboard_index)
                    .and_then(|index| SECTORS.get(index));
                painter.text(
                    rail.left_top() + Vec2::new(8.0, 8.0),
                    Align2::LEFT_TOP,
                    selected.map_or("SELECT COMMAND", |sector| sector.action.label()),
                    font.clone(),
                    selected.map_or(Color32::WHITE, |sector| sector.color),
                );
                painter.text(
                    rail.left_bottom() + Vec2::new(8.0, -7.0),
                    Align2::LEFT_BOTTOM,
                    "LMB TO EXECUTE",
                    font,
                    hud::CYAN,
                );
                response
            })
            .inner;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Tactical file actions")
        });

        context.input_mut(|input| {
            if input.pointer.secondary_clicked() && !ignore_opening_secondary_click {
                chosen = Some(TacticalArcOutcome::Dismiss);
            }
            if input.pointer.primary_clicked()
                && let Some(pointer) = input.pointer.interact_pos()
            {
                chosen = Some(
                    self.geometry
                        .hit_test(pointer)
                        .map_or(TacticalArcOutcome::Dismiss, TacticalArcOutcome::Action),
                );
            }
            let forward = input.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                || input.consume_key(egui::Modifiers::NONE, Key::ArrowRight)
                || input.consume_key(egui::Modifiers::NONE, Key::Tab);
            let backward = input.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                || input.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                || input.consume_key(egui::Modifiers::SHIFT, Key::Tab);
            if forward {
                self.keyboard_index = Some(
                    self.keyboard_index
                        .map_or(0, |index| (index + 1) % SECTORS.len()),
                );
            }
            if backward {
                self.keyboard_index =
                    Some(self.keyboard_index.map_or(SECTORS.len() - 1, |index| {
                        (index + SECTORS.len() - 1) % SECTORS.len()
                    }));
            }
            if input.consume_key(egui::Modifiers::NONE, Key::Num1) {
                self.keyboard_index = Some(0);
            }
            if input.consume_key(egui::Modifiers::NONE, Key::Num2) {
                self.keyboard_index = Some(1);
            }
            if input.consume_key(egui::Modifiers::NONE, Key::Num3) {
                self.keyboard_index = Some(2);
            }
            if input.consume_key(egui::Modifiers::NONE, Key::Enter)
                && let Some(index) = self.keyboard_index
            {
                chosen = Some(TacticalArcOutcome::Action(SECTORS[index].action));
            }
        });
        chosen
    }
}

fn truncate(value: &str, maximum: usize) -> String {
    if value.chars().count() <= maximum {
        return value.to_owned();
    }
    format!(
        "{}…",
        value
            .chars()
            .take(maximum.saturating_sub(1))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::primitives::*;
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    const EPSILON: f32 = 1.0e-6;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn grapheme_width(value: &str) -> f32 {
        value.graphemes(true).count() as f32
    }

    fn non_uniform_width(value: &str) -> f32 {
        value
            .graphemes(true)
            .map(|grapheme| if grapheme == "W" { 4.0 } else { 1.0 })
            .sum()
    }

    fn relative_luminance(color: Color32) -> f32 {
        fn linearize(channel: u8) -> f32 {
            let channel = f32::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        0.2126 * linearize(color.r())
            + 0.7152 * linearize(color.g())
            + 0.0722 * linearize(color.b())
    }

    fn contrast_ratio(first: Color32, second: Color32) -> f32 {
        let first = relative_luminance(first);
        let second = relative_luminance(second);
        let lighter = first.max(second);
        let darker = first.min(second);
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn reactor_beat_hits_every_exact_keyframe() {
        for (phase, expected) in [
            (0.0, 0.0),
            (0.12, 1.0),
            (0.22, 0.0),
            (0.34, 1.0),
            (0.45, 0.0),
            (1.0, 0.0),
        ] {
            assert_close(reactor_beat(phase * REACTOR_BEAT_SECONDS), expected);
        }
    }

    #[test]
    fn reactor_beat_uses_smoothstep_and_stays_normalized() {
        assert_close(reactor_beat(0.06 * REACTOR_BEAT_SECONDS), 0.5);
        assert_close(reactor_beat(0.17 * REACTOR_BEAT_SECONDS), 0.5);
        assert_close(reactor_beat(0.28 * REACTOR_BEAT_SECONDS), 0.5);

        for sample in -2700..=2700 {
            let intensity = reactor_beat(sample as f32 / 1000.0);
            assert!(
                (0.0..=1.0).contains(&intensity),
                "sample {sample}: {intensity}"
            );
        }
    }

    #[test]
    fn reactor_beat_has_a_quiet_rest_phase() {
        for phase in [0.45, 0.7, 0.999_999] {
            assert_close(reactor_beat(phase * REACTOR_BEAT_SECONDS), 0.0);
        }
        assert_close(reactor_beat(-0.01 * REACTOR_BEAT_SECONDS), 0.0);
    }

    #[test]
    fn reactor_beat_wraps_at_the_loop_boundary() {
        assert_close(reactor_beat(REACTOR_BEAT_SECONDS), 0.0);
        assert_close(reactor_beat(REACTOR_BEAT_SECONDS * 2.0), 0.0);
        assert_close(
            reactor_beat(REACTOR_BEAT_SECONDS * 1.06),
            reactor_beat(REACTOR_BEAT_SECONDS * 0.06),
        );
    }

    #[test]
    fn active_style_scales_the_label_and_caps_bloom_width() {
        let baseline = active_sector_style(ACTIVE_CYAN, 0.0, 0.75);
        let peak = active_sector_style(ACTIVE_CYAN, 1.0, 0.75);
        assert_close(baseline.label_scale, 1.0);
        assert_close(peak.label_scale, 1.095);
        assert_eq!(baseline.inner_stroke.color, ACTIVE_CYAN);
        assert_close(baseline.inner_stroke.width, 2.0 * 0.75);
        assert!(baseline.outer_bloom.width <= 5.0 * 0.75);
        assert_close(peak.outer_bloom.width, 5.0 * 0.75);

        let clamped = active_sector_style(ACTIVE_CYAN, 10.0, 1.0);
        assert_close(clamped.label_scale, 1.095);
        assert_close(clamped.outer_bloom.width, 5.0);
    }

    #[test]
    fn active_tokens_are_exact_opaque_colors_with_accessible_label_contrast() {
        assert_eq!(ACTIVE_CYAN.to_array(), [0x1e, 0xcd, 0xe2, 0xff]);
        assert_eq!(ACTIVE_LIME.to_array(), [0xbd, 0xff, 0x3e, 0xff]);
        assert_eq!(ACTIVE_MAGENTA.to_array(), [0xff, 0x49, 0xbc, 0xff]);
        assert_eq!(ACTIVE_LABEL_INK.to_array(), [0x07, 0x09, 0x0a, 0xff]);

        for color in [ACTIVE_CYAN, ACTIVE_LIME, ACTIVE_MAGENTA] {
            for phase in [0.0, 0.12, 0.34] {
                let intensity = reactor_beat(phase * REACTOR_BEAT_SECONDS);
                let style = active_sector_style(color, intensity, 1.0);
                assert_eq!(style.fill.a(), 255);
                assert_eq!(style.label_color, ACTIVE_LABEL_INK);
                assert!(
                    contrast_ratio(style.fill, style.label_color) >= 4.5,
                    "insufficient contrast for {color:?}"
                );
            }
        }
    }

    #[test]
    fn end_ellipsis_preserves_unicode_graphemes_and_minimum_name_prefix() {
        let value = "👨‍👩‍👧‍👦e\u{301}東京資料";

        let fitted = end_ellipsis(value, 5.0, 4, grapheme_width)
            .expect("four leading graphemes and the ellipsis fit");

        assert_eq!(fitted, "👨‍👩‍👧‍👦e\u{301}東京…");
        assert_eq!(fitted.graphemes(true).count(), 5);
        assert!(end_ellipsis(value, 4.0, 4, grapheme_width).is_none());
    }

    #[test]
    fn middle_ellipsis_preserves_path_root_and_final_component() {
        let path = "C:\\用戶\\👨‍👩‍👧‍👦\\資料\\報告.txt";
        let root = "C:\\";
        let final_component = "\\報告.txt";

        let fitted = middle_ellipsis(path, root, final_component, 12.0, grapheme_width)
            .expect("the root, ellipsis, and final component fit");

        assert_eq!(fitted, "C:\\…料\\報告.txt");
        assert!(fitted.starts_with(root));
        assert!(fitted.ends_with(final_component));
        assert!(fitted.graphemes(true).count() <= 12);
    }

    #[test]
    fn middle_ellipsis_has_logarithmic_measurements_and_subquadratic_measured_volume() {
        let optional = "界".repeat(4096);
        let path = format!("C:\\{optional}\\final.txt");
        let root = "C:\\";
        let final_component = "\\final.txt";
        let maximum_width = grapheme_width(&format!("{root}…{final_component}"));
        let measurement_count = Cell::new(0);
        let measured_graphemes = Cell::new(0);
        let measured_bytes = Cell::new(0);

        let fitted = middle_ellipsis(&path, root, final_component, maximum_width, |candidate| {
            measurement_count.set(measurement_count.get() + 1);
            let graphemes = candidate.graphemes(true).count();
            measured_graphemes.set(measured_graphemes.get() + graphemes);
            measured_bytes.set(measured_bytes.get() + candidate.len());
            graphemes as f32
        })
        .expect("the mandatory root and final component fit");

        assert_eq!(fitted, format!("{root}…{final_component}"));
        let optional_count = optional.graphemes(true).count();
        let logarithmic_probe_bound =
            usize::BITS as usize - optional_count.leading_zeros() as usize;
        let invocation_bound = logarithmic_probe_bound + 2;
        let path_graphemes = path.graphemes(true).count();
        assert!(
            measurement_count.get() <= invocation_bound
                && measured_graphemes.get() <= invocation_bound * path_graphemes
                && measured_bytes.get() <= invocation_bound * path.len(),
            "calls={}, graphemes={}, bytes={} exceeded bounds calls<={invocation_bound}, graphemes<={}, bytes<={}",
            measurement_count.get(),
            measured_graphemes.get(),
            measured_bytes.get(),
            invocation_bound * path_graphemes,
            invocation_bound * path.len(),
        );
    }

    #[test]
    fn end_ellipsis_respects_non_uniform_measured_widths() {
        let narrow = "aaaaaa";
        let wide = "WWWWWW";
        let maximum_width = 6.0;
        assert_eq!(narrow.graphemes(true).count(), wide.graphemes(true).count());

        let fitted_narrow = end_ellipsis(narrow, maximum_width, 1, non_uniform_width)
            .expect("narrow text fits without truncation");
        let fitted_wide = end_ellipsis(wide, maximum_width, 1, non_uniform_width)
            .expect("wide text fits after measured truncation");

        assert_eq!(fitted_narrow, narrow);
        assert_eq!(fitted_wide, "W…");
        assert!(fitted_wide.graphemes(true).count() < fitted_narrow.graphemes(true).count());
        assert!(non_uniform_width(&fitted_wide) <= maximum_width);
    }

    #[test]
    fn middle_ellipsis_respects_non_uniform_measured_widths() {
        let narrow_path = "C:\\aaaa\\z.txt";
        let wide_path = "C:\\WWWW\\z.txt";
        let root = "C:\\";
        let final_component = "\\z.txt";
        let maximum_width = 13.0;
        assert_eq!(
            narrow_path.graphemes(true).count(),
            wide_path.graphemes(true).count()
        );

        let narrow = middle_ellipsis(
            narrow_path,
            root,
            final_component,
            maximum_width,
            non_uniform_width,
        )
        .expect("narrow path fits");
        let wide = middle_ellipsis(
            wide_path,
            root,
            final_component,
            maximum_width,
            non_uniform_width,
        )
        .expect("wide path fits after measured truncation");

        assert_eq!(narrow, narrow_path);
        assert!(wide.contains('…'));
        assert!(wide.graphemes(true).count() < narrow.graphemes(true).count());
        assert!(non_uniform_width(&wide) <= maximum_width);
    }

    #[test]
    fn center_gaps_and_outer_dead_zones_do_not_activate_actions() {
        let arc = TacticalArcGeometry::new(Pos2::new(200.0, 200.0), 42.0, 106.0);
        assert_eq!(arc.hit_test(Pos2::new(200.0, 200.0)), None);
        assert_eq!(arc.hit_test(Pos2::new(400.0, 400.0)), None);
        let gap = arc.center + Vec2::angled(-22.0_f32.to_radians()) * 74.0;
        assert_eq!(arc.hit_test(gap), None);
    }

    #[test]
    fn clamped_arc_stays_inside_work_area_at_all_edges() {
        let area = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
        for requested in [
            area.left_top(),
            area.right_top(),
            area.left_bottom(),
            area.right_bottom(),
        ] {
            let arc = TacticalArcGeometry::clamped(requested, area, 106.0);
            assert!(area.contains(arc.bounds().min));
            assert!(area.contains(arc.bounds().max));
        }
    }

    #[test]
    fn every_action_has_accessible_name_and_shortcut() {
        for action in TacticalAction::ALL {
            assert!(!action.accessible_name().is_empty());
            assert!(!action.keyboard_hint().is_empty());
        }
    }

    #[test]
    fn area_painter_keeps_tactical_geometry_in_global_coordinates() {
        let work_area = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 720.0));
        let geometry = TacticalArcGeometry::fit(Pos2::new(420.0, 280.0), work_area, 1.0)
            .expect("test work area fits tactical arc");
        let painter_clip = geometry.bounds().expand(SAFE_MARGIN);

        let draw_geometry = geometry_for_area_painter(geometry, painter_clip);

        assert_eq!(draw_geometry.center, geometry.center);
        assert!(painter_clip.contains(draw_geometry.rail_rect().min));
        assert!(painter_clip.contains(draw_geometry.rail_rect().max));
        for index in 0..SECTORS.len() {
            assert!(painter_clip.contains(draw_geometry.action_center(index)));
        }
    }

    #[test]
    fn tactical_arc_labels_are_visible_inside_the_foreground_clip() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 720.0));
        let target = ContextTarget {
            scan_id: ScanId(1),
            generation: 1,
            node_id: NodeId(7),
            identity: FileIdentity::stable(voidspace_model::VolumeId::local_for_test(1), 7, 1),
            path: PathBuf::from(r"C:\Users\test"),
            kind: NodeKind::Directory,
            root: PathBuf::from(r"C:\"),
            view_root: NodeId(1),
            display_name: "test".to_owned(),
            display_size: "9.7G".to_owned(),
            origin_focus: Id::new("test-target"),
        };
        let mut arc = TacticalArcState::new(target, Pos2::new(1000.0, 600.0), work_area, false)
            .expect("test work area fits tactical arc");
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };
        let mut first_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(8.0));
        });
        first_output.textures_delta.clear();
        let mut output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(8.0));
        });

        fn visible_text_shapes(shape: &Shape, clip: Rect) -> usize {
            match shape {
                Shape::Vec(shapes) => shapes
                    .iter()
                    .map(|shape| visible_text_shapes(shape, clip))
                    .sum(),
                Shape::Text(_) => usize::from(clip.intersects(shape.visual_bounding_rect())),
                _ => 0,
            }
        }

        let visible_labels = output
            .shapes
            .iter()
            .map(|clipped| visible_text_shapes(&clipped.shape, clipped.clip_rect))
            .sum::<usize>();

        output.textures_delta.clear();
        assert_eq!(visible_labels, 7);
    }
}
