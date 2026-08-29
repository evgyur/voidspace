use std::{f32::consts::PI, path::PathBuf, time::Duration};

use egui::{Align2, Color32, FontId, Galley, Id, Key, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use voidspace_model::{FileIdentity, NodeId, NodeKind, ScanId};

use crate::hud;

const INNER_RADIUS: f32 = 42.0;
const OUTER_RADIUS: f32 = 106.0;
const HUB_RADIUS: f32 = 30.0;
const SECTOR_HALF_ANGLE: f32 = 18.0_f32.to_radians();
const ACTION_TAG_RADIUS: f32 = 112.0;
const ACTION_TAG_WIDTH: f32 = 76.0;
const ACTION_TAG_HEIGHT: f32 = 24.0;
const ACTION_TAG_FONT_POINTS: f32 = 11.0;
const ACTION_TAG_PEAK_SCALE: f32 = 1.095;
const ACTION_TAG_SHADOW_BLUR: f32 = 22.0;
const FAN_VISUAL_EXTENT: f32 = ACTION_TAG_RADIUS
    + ACTION_TAG_WIDTH * 0.5 * ACTION_TAG_PEAK_SCALE
    + ACTION_TAG_SHADOW_BLUR * 0.5;
const TARGET_PLATE_WIDTH: f32 = 280.0;
const TARGET_PLATE_HEIGHT: f32 = 94.0;
const TARGET_PLATE_GAP: f32 = 12.0;
const TARGET_PLATE_INSET: f32 = 12.0;
const TARGET_PLATE_IDENTITY_GAP: f32 = 8.0;
const TARGET_PLATE_TAG: &str = "OBJECT / ACTION TARGET";
const TARGET_PLATE_TAG_CENTER_Y: f32 = 15.0;
const TARGET_PLATE_IDENTITY_CENTER_Y: f32 = 36.0;
const TARGET_PLATE_PATH_CENTER_Y: f32 = 56.0;
const TARGET_PLATE_SEPARATOR_Y: f32 = 68.0;
const TARGET_PLATE_ACTION_CENTER_Y: f32 = 79.0;
const COMPACT_SCALE: f32 = 0.75;
const SAFE_MARGIN: f32 = 8.0;
const TARGET_PLATE_BACKGROUND: Color32 = Color32::from_rgb(0x0b, 0x0f, 0x11);
const TARGET_PLATE_BORDER: Color32 = Color32::from_rgb(0x3a, 0x44, 0x49);
const TARGET_PLATE_MUTED: Color32 = Color32::from_rgb(0x8e, 0x94, 0x9d);
const MODAL_SCRIM: Color32 = Color32::from_black_alpha(184);
const REACTOR_HEAT: Color32 = Color32::from_rgb(0xff, 0x5a, 0x22);

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
        pub(super) tag_fill: Color32,
        pub(super) inner_stroke: Stroke,
        pub(super) outer_bloom: Stroke,
        pub(super) label_color: Color32,
        pub(super) label_scale: f32,
        pub(super) tag_scale: f32,
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
        let label_intensity = (intensity * 8.0).round() / 8.0;
        let geometry_scale = geometry_scale.max(0.0);
        let fill = Color32::from_rgb(semantic_color.r(), semantic_color.g(), semantic_color.b());
        let brightness = 1.0 + 0.35 * intensity;
        let brighten =
            |channel: u8| (f32::from(channel) * brightness).round().clamp(0.0, 255.0) as u8;
        let tag_fill =
            Color32::from_rgb(brighten(fill.r()), brighten(fill.g()), brighten(fill.b()));
        let bloom_alpha = (72.0 + 112.0 * intensity).round() as u8;
        let bloom_color =
            Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), bloom_alpha);
        ActiveSectorStyle {
            fill,
            tag_fill,
            inner_stroke: Stroke::new(2.0 * geometry_scale, fill),
            outer_bloom: Stroke::new((2.0 + 3.0 * intensity) * geometry_scale, bloom_color),
            label_color: ACTIVE_LABEL_INK,
            label_scale: 1.0 + (super::ACTION_TAG_PEAK_SCALE - 1.0) * label_intensity,
            tag_scale: 1.0 + (super::ACTION_TAG_PEAK_SCALE - 1.0) * intensity,
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
    compact_label: &'static str,
    label: &'static str,
    accessible_label: &'static str,
    color: Color32,
}

const SECTORS: [SectorSpec; 3] = [
    SectorSpec {
        action: TacticalAction::OpenInExplorer,
        compact_label: "1  OPEN",
        label: "OPEN IN EXPLORER",
        accessible_label: "Open in Explorer · key 1 to select · Enter to execute",
        color: ACTIVE_CYAN,
    },
    SectorSpec {
        action: TacticalAction::Recycle,
        compact_label: "2  BIN",
        label: "MOVE TO RECYCLE BIN",
        accessible_label: "Move to Recycle Bin · key 2 to select · Enter to execute",
        color: ACTIVE_LIME,
    },
    SectorSpec {
        action: TacticalAction::DeletePermanently,
        compact_label: "3  VOID",
        label: "DELETE WITHOUT RECOVERY",
        accessible_label: "Delete without recovery · key 3 to select · Enter to execute",
        color: ACTIVE_MAGENTA,
    },
];

impl TacticalAction {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::OpenInExplorer, Self::Recycle, Self::DeletePermanently];

    pub const fn label(self) -> &'static str {
        SECTORS[self.index()].label
    }

    #[cfg(test)]
    pub const fn accessible_name(self) -> &'static str {
        match self {
            Self::OpenInExplorer => "Open in Explorer",
            Self::Recycle => "Move to Recycle Bin",
            Self::DeletePermanently => "Delete without recovery",
        }
    }

    #[cfg(test)]
    pub const fn keyboard_hint(self) -> &'static str {
        match self {
            Self::OpenInExplorer => "1",
            Self::Recycle => "2",
            Self::DeletePermanently => "3",
        }
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
        let composition_width =
            FAN_VISUAL_EXTENT + OUTER_RADIUS + TARGET_PLATE_GAP + TARGET_PLATE_WIDTH;
        let composition_height = (OUTER_RADIUS * 2.0).max(TARGET_PLATE_HEIGHT);
        let full_width = composition_width * preferred_scale + SAFE_MARGIN * 2.0;
        let full_height = composition_height * preferred_scale + SAFE_MARGIN * 2.0;
        let scale = if area.width() >= full_width && area.height() >= full_height {
            preferred_scale
        } else {
            COMPACT_SCALE
        };
        let minimum_width = composition_width * scale + SAFE_MARGIN * 2.0;
        let minimum_height = composition_height * scale + SAFE_MARGIN * 2.0;
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
        let fan_visual_extent = FAN_VISUAL_EXTENT * scale;
        let plate = (OUTER_RADIUS + TARGET_PLATE_GAP + TARGET_PLATE_WIDTH) * scale;
        let (left_extent, right_extent) = match orientation {
            Orientation::Right => (plate, fan_visual_extent),
            Orientation::Left => (fan_visual_extent, plate),
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
        let mut bounds = circle.union(self.target_plate_rect());
        for index in 0..SECTORS.len() {
            bounds = bounds.union(self.action_tag_visual_rect(index));
        }
        bounds
    }

    fn target_plate_rect(self) -> Rect {
        let size = Vec2::new(TARGET_PLATE_WIDTH, TARGET_PLATE_HEIGHT) * self.scale;
        let x = match self.orientation {
            Orientation::Right => {
                self.center.x - OUTER_RADIUS * self.scale - size.x - TARGET_PLATE_GAP * self.scale
            }
            Orientation::Left => self.center.x + (OUTER_RADIUS + TARGET_PLATE_GAP) * self.scale,
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

    #[cfg(test)]
    fn action_center(self, index: usize) -> Pos2 {
        let radius = (INNER_RADIUS + OUTER_RADIUS) * 0.5 * self.scale;
        self.center + Vec2::angled(self.action_angle(index)) * radius
    }

    fn action_tag_rect(self, index: usize) -> Rect {
        let center =
            self.center + Vec2::angled(self.action_angle(index)) * (ACTION_TAG_RADIUS * self.scale);
        Rect::from_center_size(
            center,
            Vec2::new(ACTION_TAG_WIDTH, ACTION_TAG_HEIGHT) * self.scale,
        )
    }

    fn action_tag_visual_rect(self, index: usize) -> Rect {
        let tag = self.action_tag_rect(index);
        Rect::from_center_size(tag.center(), tag.size() * ACTION_TAG_PEAK_SCALE)
            .expand(ACTION_TAG_SHADOW_BLUR * 0.5 * self.scale)
    }

    pub fn hit_test(self, pointer: Pos2) -> Option<TacticalAction> {
        if let Some((index, _)) = SECTORS
            .iter()
            .enumerate()
            .find(|(index, _)| self.action_tag_rect(*index).contains(pointer))
        {
            return Some(SECTORS[index].action);
        }
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
    debug_assert!(
        geometry
            .work_area
            .contains(geometry.bounds().expand(SAFE_MARGIN).min)
    );
    debug_assert!(
        geometry
            .work_area
            .contains(geometry.bounds().expand(SAFE_MARGIN).max)
    );
    debug_assert!(painter_clip.intersects(geometry.bounds()));
    geometry
}

#[derive(Clone, Debug)]
struct TargetPlateText {
    fitted_name: String,
    fitted_path: String,
    fonts: TargetPlateFonts,
}

#[derive(Clone, Debug, PartialEq)]
struct TargetPlateFonts {
    tag: FontId,
    size: FontId,
    name: FontId,
    small: FontId,
}

#[derive(Clone, Debug, PartialEq)]
struct TargetPlateTextCacheKey {
    fonts: TargetPlateFonts,
    geometry_scale_bits: u32,
}

#[derive(Clone, Debug)]
struct TargetPlateTextCache {
    key: TargetPlateTextCacheKey,
    text: TargetPlateText,
}

fn scaled_font(font: &FontId, points: f32, scale: f32) -> FontId {
    FontId::new(points * scale, font.family.clone())
}

fn target_plate_fonts(font: &FontId, scale: f32) -> TargetPlateFonts {
    TargetPlateFonts {
        tag: scaled_font(font, 8.0, scale),
        size: scaled_font(font, 13.0, scale),
        name: scaled_font(font, 20.0, scale),
        small: scaled_font(font, 9.0, scale),
    }
}

fn galley_baseline(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(galley.size().y * 0.5)
}

fn target_path_anchors(target: &ContextTarget, path: &str) -> Option<(String, String)> {
    let prefix = target.root.to_string_lossy().into_owned();
    if !path.starts_with(&prefix) {
        return None;
    }
    if target.path == target.root {
        return Some((prefix, String::new()));
    }

    let Some(final_component) = target.path.file_name() else {
        return Some((prefix, String::new()));
    };
    let final_component = final_component.to_string_lossy();
    let final_start = path.rfind(final_component.as_ref())?;
    let mut suffix_start = final_start;
    if let Some((separator_start, separator)) = path[..final_start].char_indices().next_back()
        && matches!(separator, '\\' | '/')
        && separator_start >= prefix.len()
    {
        suffix_start = separator_start;
    }
    Some((prefix, path[suffix_start..].to_owned()))
}

fn fit_target_plate_text(
    painter: &egui::Painter,
    target: &ContextTarget,
    full_path: &str,
    plate: Rect,
    scale: f32,
    fonts: &TargetPlateFonts,
) -> Option<TargetPlateText> {
    let content_width = plate.width() - TARGET_PLATE_INSET * 2.0 * scale;
    let size_width = painter
        .layout_no_wrap(
            target.display_size.clone(),
            fonts.size.clone(),
            Color32::WHITE,
        )
        .size()
        .x;
    let name_width = content_width - size_width - TARGET_PLATE_IDENTITY_GAP * scale;
    if name_width <= 0.0 {
        return None;
    }
    let fitted_name = primitives::end_ellipsis(&target.display_name, name_width, 4, |value| {
        painter
            .layout_no_wrap(value.to_owned(), fonts.name.clone(), Color32::WHITE)
            .size()
            .x
    })?;

    let (prefix, suffix) = target_path_anchors(target, full_path)?;
    let fitted_path =
        primitives::middle_ellipsis(full_path, &prefix, &suffix, content_width, |value| {
            painter
                .layout_no_wrap(value.to_owned(), fonts.small.clone(), Color32::WHITE)
                .size()
                .x
        })?;

    Some(TargetPlateText {
        fitted_name,
        fitted_path,
        fonts: fonts.clone(),
    })
}

fn target_plate_response_id(target: &ContextTarget) -> Id {
    Id::new("tactical-target-plate").with((target.scan_id, target.node_id, target.generation))
}

fn target_plate_description(
    target: &ContextTarget,
    path: &str,
    action: Option<&SectorSpec>,
) -> String {
    format!(
        "Target size {}; name {}; path {}; action {}",
        target.display_size,
        target.display_name,
        path,
        action.map_or("SELECT COMMAND", |sector| sector.action.label())
    )
}

#[derive(Clone, Debug)]
struct TargetSemantics {
    path: String,
    descriptions: [String; 4],
}

impl TargetSemantics {
    fn new(target: &ContextTarget) -> Self {
        let path = target.path.display().to_string();
        let descriptions = std::array::from_fn(|index| {
            target_plate_description(
                target,
                &path,
                index.checked_sub(1).map(|index| &SECTORS[index]),
            )
        });
        Self { path, descriptions }
    }

    fn description(&self, action: Option<TacticalAction>) -> &str {
        &self.descriptions[action.map_or(0, |action| action.index() + 1)]
    }
}

#[cfg(windows)]
fn client_area_animation_enabled() -> Option<bool> {
    use windows::{
        Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        },
        core::BOOL,
    };

    let mut enabled = BOOL::default();
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL to the valid stack pointer supplied here.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    Some(enabled.as_bool())
}

#[cfg(not(windows))]
fn client_area_animation_enabled() -> Option<bool> {
    None
}

fn animation_repaint_after(
    motion_enabled: bool,
    pointer_hovered_action: Option<TacticalAction>,
) -> Option<Duration> {
    (motion_enabled && pointer_hovered_action.is_some()).then(|| Duration::from_millis(16))
}

fn paint_source_target_lock(painter: &egui::Painter, rect: Rect, geometry_scale: f32) {
    let scale = geometry_scale.max(COMPACT_SCALE);
    let accent = Color32::from_rgba_unmultiplied(0xff, 0x5a, 0x22, 176);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0 * scale, accent),
        egui::StrokeKind::Inside,
    );
    hud::paint_corner_brackets(painter, rect.shrink(3.0 * scale), accent);
}

#[derive(Clone, Debug)]
pub struct TacticalArcState {
    target: ContextTarget,
    pub geometry: TacticalArcGeometry,
    source_rect: Option<Rect>,
    keyboard_index: Option<usize>,
    focus_slot: Option<ModalFocusSlot>,
    focus_request_pending: bool,
    pointer_hovered_action: Option<TacticalAction>,
    pointer_beat_started_at: Option<f64>,
    motion_enabled: bool,
    armed: bool,
    target_semantics: TargetSemantics,
    target_plate_text_cache: Option<TargetPlateTextCache>,
    #[cfg(test)]
    target_plate_fit_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModalFocusSlot {
    Action(usize),
    Plate,
}

impl ModalFocusSlot {
    const COUNT: usize = 4;

    fn index(self) -> usize {
        match self {
            Self::Action(index) => index,
            Self::Plate => 3,
        }
    }

    fn from_index(index: usize) -> Self {
        if index == 3 {
            Self::Plate
        } else {
            Self::Action(index)
        }
    }
}

impl TacticalArcState {
    #[cfg(test)]
    pub fn new(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
    ) -> Option<Self> {
        Self::new_with_motion_query_impl(
            target,
            pointer,
            work_area,
            keyboard_open,
            None,
            client_area_animation_enabled,
        )
    }

    pub fn new_with_source_rect(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
        source_rect: Rect,
    ) -> Option<Self> {
        Self::new_with_motion_query_impl(
            target,
            pointer,
            work_area,
            keyboard_open,
            Some(source_rect),
            client_area_animation_enabled,
        )
    }

    #[cfg(test)]
    fn new_with_motion_query(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
        query: impl FnOnce() -> Option<bool>,
    ) -> Option<Self> {
        Self::new_with_motion_query_impl(target, pointer, work_area, keyboard_open, None, query)
    }

    #[cfg(test)]
    fn new_with_source_rect_and_motion_query(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
        source_rect: Rect,
        query: impl FnOnce() -> Option<bool>,
    ) -> Option<Self> {
        Self::new_with_motion_query_impl(
            target,
            pointer,
            work_area,
            keyboard_open,
            Some(source_rect),
            query,
        )
    }

    fn new_with_motion_query_impl(
        target: ContextTarget,
        pointer: Pos2,
        work_area: Rect,
        keyboard_open: bool,
        source_rect: Option<Rect>,
        query: impl FnOnce() -> Option<bool>,
    ) -> Option<Self> {
        let geometry = TacticalArcGeometry::fit(pointer, work_area, 1.0)?;
        let target_semantics = TargetSemantics::new(&target);
        Some(Self {
            target,
            geometry,
            source_rect,
            keyboard_index: keyboard_open.then_some(0),
            focus_slot: keyboard_open.then_some(ModalFocusSlot::Action(0)),
            focus_request_pending: keyboard_open,
            pointer_hovered_action: None,
            pointer_beat_started_at: None,
            motion_enabled: query().unwrap_or(false),
            armed: false,
            target_semantics,
            target_plate_text_cache: None,
            #[cfg(test)]
            target_plate_fit_count: 0,
        })
    }

    pub fn target(&self) -> &ContextTarget {
        &self.target
    }

    pub fn into_target(self) -> ContextTarget {
        self.target
    }

    fn action_response_id(index: usize) -> Id {
        Id::new("tactical-action").with(index)
    }

    fn focus_id(&self, slot: ModalFocusSlot) -> Id {
        match slot {
            ModalFocusSlot::Action(index) => Self::action_response_id(index),
            ModalFocusSlot::Plate => target_plate_response_id(&self.target),
        }
    }

    fn queue_focus(&mut self, slot: ModalFocusSlot) {
        self.focus_slot = Some(slot);
        self.focus_request_pending = true;
    }

    fn cycle_focus(&mut self, forward: bool) {
        let slot = if let Some(current) = self.focus_slot {
            let next = if forward {
                (current.index() + 1) % ModalFocusSlot::COUNT
            } else {
                (current.index() + ModalFocusSlot::COUNT - 1) % ModalFocusSlot::COUNT
            };
            ModalFocusSlot::from_index(next)
        } else if forward {
            ModalFocusSlot::Action(0)
        } else {
            ModalFocusSlot::Plate
        };
        if let ModalFocusSlot::Action(index) = slot {
            self.keyboard_index = Some(index);
        }
        self.queue_focus(slot);
    }

    fn select_keyboard_action(&mut self, index: usize) {
        self.keyboard_index = Some(index);
        self.queue_focus(ModalFocusSlot::Action(index));
    }

    fn update_pointer_hover(&mut self, hovered: Option<TacticalAction>, now: f64) {
        if self.pointer_hovered_action == hovered {
            return;
        }
        self.pointer_hovered_action = hovered;
        self.pointer_beat_started_at = hovered.map(|_| now);
    }

    fn visual_active_action(&self) -> Option<TacticalAction> {
        self.pointer_hovered_action.or_else(|| {
            self.keyboard_index
                .and_then(|index| SECTORS.get(index))
                .map(|sector| sector.action)
        })
    }

    fn prepare_target_plate_text(
        &mut self,
        painter: &egui::Painter,
        plate: Rect,
        geometry_scale: f32,
        font: &FontId,
    ) -> bool {
        let fonts = target_plate_fonts(font, geometry_scale);
        let key = TargetPlateTextCacheKey {
            fonts: fonts.clone(),
            geometry_scale_bits: geometry_scale.to_bits(),
        };
        if self
            .target_plate_text_cache
            .as_ref()
            .is_some_and(|cached| cached.key == key)
        {
            return true;
        }

        #[cfg(test)]
        {
            self.target_plate_fit_count += 1;
        }
        self.target_plate_text_cache = fit_target_plate_text(
            painter,
            &self.target,
            &self.target_semantics.path,
            plate,
            geometry_scale,
            &fonts,
        )
        .map(|text| TargetPlateTextCache { key, text });
        self.target_plate_text_cache.is_some()
    }

    pub fn resolve_input(&mut self, context: &egui::Context) -> Option<TacticalArcOutcome> {
        let opening_frame = !self.armed;
        let geometry = self.geometry;
        let pointer = context.pointer_hover_pos();
        let hovered = pointer.and_then(|position| geometry.hit_test(position));
        let now = context.input(|input| input.time);
        self.update_pointer_hover(hovered, now);
        let focused = context.memory(|memory| memory.focused());
        if let Some(slot) = (0..ModalFocusSlot::COUNT)
            .map(ModalFocusSlot::from_index)
            .find(|slot| Some(self.focus_id(*slot)) == focused)
        {
            self.focus_slot = Some(slot);
        } else if !self.focus_request_pending
            && focused.is_some()
            && let Some(slot) = self.focus_slot
        {
            self.queue_focus(slot);
        }

        let (
            secondary_clicked,
            primary_clicked,
            interact_pos,
            action_forward,
            action_backward,
            tab_forward,
            tab_backward,
            number,
            enter,
            escape,
        ) = context.input_mut(|input| {
            let action_forward = input.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                || input.consume_key(egui::Modifiers::NONE, Key::ArrowRight);
            let action_backward = input.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                || input.consume_key(egui::Modifiers::NONE, Key::ArrowLeft);
            let tab_backward = input.consume_key(egui::Modifiers::SHIFT, Key::Tab);
            let tab_forward = input.consume_key(egui::Modifiers::NONE, Key::Tab);
            let number = if input.consume_key(egui::Modifiers::NONE, Key::Num1) {
                Some(0)
            } else if input.consume_key(egui::Modifiers::NONE, Key::Num2) {
                Some(1)
            } else if input.consume_key(egui::Modifiers::NONE, Key::Num3) {
                Some(2)
            } else {
                None
            };
            (
                input.pointer.secondary_clicked(),
                input.pointer.primary_clicked(),
                input.pointer.interact_pos(),
                action_forward,
                action_backward,
                tab_forward,
                tab_backward,
                number,
                input.consume_key(egui::Modifiers::NONE, Key::Enter),
                input.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });

        if tab_forward {
            self.cycle_focus(true);
        } else if tab_backward {
            self.cycle_focus(false);
        }
        if action_forward {
            let index = self
                .keyboard_index
                .map_or(0, |index| (index + 1) % SECTORS.len());
            self.select_keyboard_action(index);
        } else if action_backward {
            let index = self.keyboard_index.map_or(SECTORS.len() - 1, |index| {
                (index + SECTORS.len() - 1) % SECTORS.len()
            });
            self.select_keyboard_action(index);
        }
        if let Some(index) = number {
            self.select_keyboard_action(index);
        }

        let mut chosen = escape.then_some(TacticalArcOutcome::Dismiss);
        if secondary_clicked && !opening_frame {
            chosen = Some(TacticalArcOutcome::Dismiss);
        }
        if !secondary_clicked {
            self.armed = true;
        }
        if primary_clicked && let Some(pointer) = interact_pos {
            chosen = Some(
                geometry
                    .hit_test(pointer)
                    .map_or(TacticalArcOutcome::Dismiss, TacticalArcOutcome::Action),
            );
        }
        if enter && let Some(index) = self.keyboard_index {
            chosen = Some(TacticalArcOutcome::Action(SECTORS[index].action));
        }
        chosen
    }

    pub fn paint(&mut self, context: &egui::Context, font: FontId) -> Option<TacticalArcOutcome> {
        let mut plate_text_fit_failed = false;
        let geometry = self.geometry;
        let content_rect = context.content_rect();
        let pointer = context.pointer_hover_pos();
        let hovered = pointer.and_then(|position| geometry.hit_test(position));
        let now = context.input(|input| input.time);
        self.update_pointer_hover(hovered, now);
        let visual_active_action = self.visual_active_action();
        let pointer_beat_intensity = if self.motion_enabled && hovered.is_some() {
            self.pointer_beat_started_at.map_or(0.0, |started_at| {
                primitives::reactor_beat((now - started_at) as f32)
            })
        } else {
            0.0
        };
        if let Some(delay) = animation_repaint_after(self.motion_enabled, hovered) {
            context.request_repaint_after(delay);
        }
        let modal_area = egui::Area::new(Id::new("tactical-arc"))
            .order(egui::Order::Foreground)
            .sense(Sense::CLICK)
            .fade_in(false)
            .default_size(content_rect.size())
            .fixed_pos(content_rect.min);
        modal_area.show(context, |ui| {
            ui.set_min_size(content_rect.size());
            let (rect, _) = ui.allocate_exact_size(content_rect.size(), Sense::CLICK);
            ui.set_clip_rect(content_rect);
            let painter = if ui.is_visible() {
                ui.painter_at(rect)
            } else {
                context
                    .layer_painter(ui.layer_id())
                    .with_clip_rect(content_rect)
            };
            let draw_geometry = geometry_for_area_painter(geometry, rect);
            let center = draw_geometry.center;
            painter.rect_filled(rect, 0.0, MODAL_SCRIM);
            if let Some(source_rect) = self.source_rect {
                let source_rect = source_rect.intersect(content_rect);
                if source_rect.width() > 2.0
                    && source_rect.height() > 2.0
                    && !source_rect.intersects(draw_geometry.bounds().expand(6.0))
                {
                    paint_source_target_lock(&painter, source_rect, draw_geometry.scale);
                }
            }
            if draw_geometry.origin.distance(center) > 2.0 {
                painter.line_segment(
                    [draw_geometry.origin, center],
                    Stroke::new(
                        if self.source_rect.is_some() {
                            1.75
                        } else {
                            1.0
                        },
                        if self.source_rect.is_some() {
                            REACTOR_HEAT
                        } else {
                            hud::ORANGE
                        },
                    ),
                );
            }
            if let Some(pointer) = pointer {
                painter.line_segment([center, pointer], Stroke::new(1.5, hud::CYAN));
            }

            for (index, sector) in SECTORS.iter().enumerate() {
                let active = visual_active_action == Some(sector.action);
                let angle = draw_geometry.action_angle(index);
                let mut outer_points = Vec::with_capacity(13);
                let mut inner_points = Vec::with_capacity(13);
                for step in 0..=12 {
                    let fraction = step as f32 / 12.0;
                    let sample = angle - SECTOR_HALF_ANGLE + SECTOR_HALF_ANGLE * 2.0 * fraction;
                    outer_points
                        .push(center + Vec2::angled(sample) * (OUTER_RADIUS * draw_geometry.scale));
                    inner_points
                        .push(center + Vec2::angled(sample) * (INNER_RADIUS * draw_geometry.scale));
                }
                let active_style = active.then(|| {
                    let intensity = if hovered == Some(sector.action) {
                        pointer_beat_intensity
                    } else {
                        0.0
                    };
                    primitives::active_sector_style(sector.color, intensity, draw_geometry.scale)
                });
                let fill = if let Some(style) = active_style {
                    style.fill
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
                let mut boundary = outer_points;
                boundary.extend(inner_points.into_iter().rev());
                if let Some(style) = active_style {
                    painter.add(Shape::closed_line(boundary.clone(), style.outer_bloom));
                    painter.add(Shape::closed_line(boundary, style.inner_stroke));
                } else {
                    painter.add(Shape::closed_line(boundary, Stroke::new(1.0, sector.color)));
                }
                let action_tag = draw_geometry.action_tag_rect(index);
                let painted_action_tag = active_style.map_or(action_tag, |style| {
                    Rect::from_center_size(action_tag.center(), action_tag.size() * style.tag_scale)
                });
                let (label_font, label_color) = active_style.map_or_else(
                    || {
                        (
                            FontId::new(
                                ACTION_TAG_FONT_POINTS * draw_geometry.scale,
                                font.family.clone(),
                            ),
                            sector.color,
                        )
                    },
                    |style| {
                        (
                            FontId::new(
                                ACTION_TAG_FONT_POINTS * draw_geometry.scale * style.label_scale,
                                font.family.clone(),
                            ),
                            style.label_color,
                        )
                    },
                );
                if let Some(style) = active_style {
                    let shadow = egui::epaint::Shadow {
                        offset: [0, 0],
                        blur: (ACTION_TAG_SHADOW_BLUR * draw_geometry.scale)
                            .round()
                            .clamp(0.0, 255.0) as u8,
                        spread: 0,
                        color: style.tag_fill,
                    };
                    painter.add(Shape::Rect(shadow.as_shape(painted_action_tag, 0)));
                    painter.rect_filled(painted_action_tag, 0.0, style.tag_fill);
                    painter.rect_stroke(
                        painted_action_tag,
                        0.0,
                        style.inner_stroke,
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.line_segment(
                        [
                            center + Vec2::angled(angle) * (OUTER_RADIUS * draw_geometry.scale),
                            action_tag.center(),
                        ],
                        Stroke::new(draw_geometry.scale, sector.color),
                    );
                }
                painter.text(
                    painted_action_tag.center(),
                    Align2::CENTER_CENTER,
                    sector.compact_label,
                    label_font,
                    label_color,
                );
                let action_response =
                    ui.interact(action_tag, Self::action_response_id(index), Sense::click());
                if self.focus_request_pending
                    && self.focus_slot == Some(ModalFocusSlot::Action(index))
                {
                    action_response.request_focus();
                    self.focus_request_pending = false;
                }
                action_response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        sector.accessible_label,
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
                "TARGET",
                font.clone(),
                hud::ORANGE,
            );
            painter.text(
                center + Vec2::new(0.0, 4.0),
                Align2::CENTER_TOP,
                "LOCKED",
                font.clone(),
                Color32::WHITE,
            );

            let plate = draw_geometry.target_plate_rect();
            if !self.prepare_target_plate_text(&painter, plate, draw_geometry.scale, &font) {
                plate_text_fit_failed = true;
                return;
            }
            let plate_text = &self
                .target_plate_text_cache
                .as_ref()
                .expect("successful target-plate fit must be cached")
                .text;
            painter.rect_filled(plate, 0.0, TARGET_PLATE_BACKGROUND);
            painter.rect_stroke(
                plate,
                0.0,
                Stroke::new(draw_geometry.scale, TARGET_PLATE_BORDER),
                egui::StrokeKind::Inside,
            );
            let selected = visual_active_action
                .map(TacticalAction::index)
                .and_then(|index| SECTORS.get(index));
            let inset = TARGET_PLATE_INSET * draw_geometry.scale;
            let text_x = plate.left() + inset;
            painter.text(
                Pos2::new(
                    text_x,
                    plate.top() + TARGET_PLATE_TAG_CENTER_Y * draw_geometry.scale,
                ),
                Align2::LEFT_CENTER,
                TARGET_PLATE_TAG,
                plate_text.fonts.tag.clone(),
                hud::ORANGE,
            );
            let identity_y = plate.top() + TARGET_PLATE_IDENTITY_CENTER_Y * draw_geometry.scale;
            let size_galley = painter.layout_no_wrap(
                self.target.display_size.clone(),
                plate_text.fonts.size.clone(),
                hud::ORANGE,
            );
            let size_width = size_galley.size().x;
            let name_galley = painter.layout_no_wrap(
                plate_text.fitted_name.clone(),
                plate_text.fonts.name.clone(),
                Color32::WHITE,
            );
            let name_y = identity_y - name_galley.size().y * 0.5;
            let identity_baseline = name_y + galley_baseline(&name_galley);
            let size_y = identity_baseline - galley_baseline(&size_galley);
            painter.galley(Pos2::new(text_x, size_y), size_galley, hud::ORANGE);
            painter.galley(
                Pos2::new(
                    text_x + size_width + TARGET_PLATE_IDENTITY_GAP * draw_geometry.scale,
                    name_y,
                ),
                name_galley,
                Color32::WHITE,
            );
            painter.text(
                Pos2::new(
                    text_x,
                    plate.top() + TARGET_PLATE_PATH_CENTER_Y * draw_geometry.scale,
                ),
                Align2::LEFT_CENTER,
                &plate_text.fitted_path,
                plate_text.fonts.small.clone(),
                TARGET_PLATE_MUTED,
            );
            let separator_y = plate.top() + TARGET_PLATE_SEPARATOR_Y * draw_geometry.scale;
            painter.line_segment(
                [
                    Pos2::new(text_x, separator_y),
                    Pos2::new(plate.right() - inset, separator_y),
                ],
                Stroke::new(draw_geometry.scale, TARGET_PLATE_BORDER),
            );
            painter.text(
                Pos2::new(
                    text_x,
                    plate.top() + TARGET_PLATE_ACTION_CENTER_Y * draw_geometry.scale,
                ),
                Align2::LEFT_CENTER,
                selected.map_or("SELECT COMMAND", |sector| sector.action.label()),
                plate_text.fonts.small.clone(),
                selected.map_or(Color32::WHITE, |sector| sector.color),
            );
            let plate_response = ui.interact(
                plate,
                target_plate_response_id(&self.target),
                Sense::focusable_noninteractive(),
            );
            if self.focus_request_pending && self.focus_slot == Some(ModalFocusSlot::Plate) {
                plate_response.request_focus();
                self.focus_request_pending = false;
            }
            let description = self
                .target_semantics
                .description(selected.map(|sector| sector.action));
            plate_response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Window, true, description)
            });
            if plate_response.hovered()
                || plate_response.has_focus()
                || self.focus_slot == Some(ModalFocusSlot::Plate)
            {
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    plate_response.layer_id,
                    plate_response.id,
                    plate_response.rect,
                )
                .show(|ui| {
                    ui.label(&self.target.display_size);
                    ui.label(&self.target.display_name);
                    ui.label(&self.target_semantics.path);
                    ui.label(selected.map_or("SELECT COMMAND", |sector| sector.action.label()));
                });
            }
        });
        if plate_text_fit_failed {
            return Some(TacticalArcOutcome::Dismiss);
        }
        None
    }

    #[cfg(test)]
    fn show(&mut self, context: &egui::Context, font: FontId) -> Option<TacticalArcOutcome> {
        self.resolve_input(context)
            .or_else(|| self.paint(context, font))
    }
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

    fn target_fixture(
        display_size: &str,
        display_name: &str,
        path: &str,
        root: &str,
    ) -> ContextTarget {
        ContextTarget {
            scan_id: ScanId(1),
            generation: 1,
            node_id: NodeId(7),
            identity: FileIdentity::stable(voidspace_model::VolumeId::local_for_test(1), 7, 1),
            path: PathBuf::from(path),
            kind: NodeKind::Directory,
            root: PathBuf::from(root),
            view_root: NodeId(1),
            display_name: display_name.to_owned(),
            display_size: display_size.to_owned(),
            origin_focus: Id::new("test-target-fixture"),
        }
    }

    fn collect_text_shapes<'a>(shape: &'a Shape, found: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text_shapes(shape, found);
                }
            }
            Shape::Text(text) => found.push(text),
            _ => {}
        }
    }

    fn rendered_text_shapes(output: &egui::FullOutput) -> Vec<&egui::epaint::TextShape> {
        let mut text = Vec::new();
        for clipped in &output.shapes {
            collect_text_shapes(&clipped.shape, &mut text);
        }
        text
    }

    fn rendered_text<'a>(
        text: &'a [&egui::epaint::TextShape],
        label: &str,
    ) -> &'a egui::epaint::TextShape {
        text.iter()
            .copied()
            .find(|shape| shape.galley.text() == label)
            .unwrap_or_else(|| panic!("missing rendered label {label:?}"))
    }

    fn logical_text_rect(shape: &egui::epaint::TextShape) -> Rect {
        Rect::from_min_size(shape.pos, shape.galley.size())
    }

    fn rendered_font_size(shape: &egui::epaint::TextShape) -> f32 {
        shape.galley.job.sections[0].format.font_id.size
    }

    fn rendered_color(shape: &egui::epaint::TextShape) -> Color32 {
        shape.galley.job.sections[0].format.color
    }

    fn rendered_baseline(shape: &egui::epaint::TextShape) -> f32 {
        let row = &shape.galley.rows[0];
        shape.pos.y + row.pos.y + row.glyphs[0].pos.y
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

    fn arc_with_motion_query(
        query: impl FnOnce() -> Option<bool>,
        keyboard_open: bool,
    ) -> TacticalArcState {
        TacticalArcState::new_with_motion_query(
            target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\"),
            Pos2::new(320.0, 360.0),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)),
            keyboard_open,
            query,
        )
        .expect("test work area fits tactical arc")
    }

    #[test]
    fn hover_change_restarts_reactor_beat_but_same_action_does_not() {
        let mut arc = arc_with_motion_query(|| Some(true), true);

        arc.update_pointer_hover(Some(TacticalAction::Recycle), 10.0);
        assert_eq!(arc.pointer_beat_started_at, Some(10.0));
        arc.update_pointer_hover(Some(TacticalAction::Recycle), 20.0);
        assert_eq!(arc.pointer_beat_started_at, Some(10.0));

        arc.update_pointer_hover(Some(TacticalAction::DeletePermanently), 30.0);
        assert_eq!(arc.pointer_beat_started_at, Some(30.0));
    }

    #[test]
    fn pointer_visual_precedes_keyboard_then_hover_exit_restores_keyboard() {
        let mut arc = arc_with_motion_query(|| Some(true), true);
        assert_eq!(
            arc.visual_active_action(),
            Some(TacticalAction::OpenInExplorer)
        );

        arc.update_pointer_hover(Some(TacticalAction::Recycle), 10.0);
        assert_eq!(arc.visual_active_action(), Some(TacticalAction::Recycle));
        assert_eq!(
            arc.keyboard_index,
            Some(0),
            "hover must not move Enter target"
        );

        arc.update_pointer_hover(None, 20.0);
        assert_eq!(arc.pointer_beat_started_at, None);
        assert_eq!(
            arc.visual_active_action(),
            Some(TacticalAction::OpenInExplorer)
        );
        assert_eq!(arc.keyboard_index, Some(0));
    }

    #[test]
    fn client_animation_query_runs_once_and_fails_closed() {
        let query_count = Cell::new(0);
        let enabled = arc_with_motion_query(
            || {
                query_count.set(query_count.get() + 1);
                Some(true)
            },
            false,
        );
        assert!(enabled.motion_enabled);
        assert_eq!(query_count.get(), 1);

        assert!(!arc_with_motion_query(|| Some(false), false).motion_enabled);
        assert!(!arc_with_motion_query(|| None, false).motion_enabled);
    }

    #[test]
    fn animation_repaint_policy_requires_enabled_pointer_hover() {
        assert_eq!(
            animation_repaint_after(true, Some(TacticalAction::Recycle)),
            Some(std::time::Duration::from_millis(16))
        );
        assert_eq!(
            animation_repaint_after(false, Some(TacticalAction::Recycle)),
            None
        );
        assert_eq!(animation_repaint_after(true, None), None);
        assert_eq!(animation_repaint_after(false, None), None);
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
    fn active_style_uses_semantic_reactor_beat_color_and_caps_bloom_width() {
        let baseline = active_sector_style(ACTIVE_CYAN, 0.0, 0.75);
        let peak = active_sector_style(ACTIVE_CYAN, 1.0, 0.75);
        assert_close(baseline.label_scale, 1.0);
        assert_close(peak.label_scale, 1.095);
        assert_close(baseline.tag_scale, 1.0);
        assert_close(peak.tag_scale, ACTION_TAG_PEAK_SCALE);
        assert_eq!(baseline.fill, ACTIVE_CYAN);
        assert_eq!(baseline.tag_fill, ACTIVE_CYAN);
        assert_eq!(peak.tag_fill, Color32::from_rgb(0x29, 0xff, 0xff));
        assert_eq!(baseline.inner_stroke.color, ACTIVE_CYAN);
        for bloom in [baseline.outer_bloom.color, peak.outer_bloom.color] {
            let [red, green, blue, _] = bloom.to_srgba_unmultiplied();
            assert!(red.abs_diff(ACTIVE_CYAN.r()) <= 2);
            assert!(green.abs_diff(ACTIVE_CYAN.g()) <= 2);
            assert!(blue.abs_diff(ACTIVE_CYAN.b()) <= 2);
        }
        assert!(peak.outer_bloom.color.a() > baseline.outer_bloom.color.a());
        assert_close(baseline.inner_stroke.width, 2.0 * 0.75);
        assert!(baseline.outer_bloom.width <= 5.0 * 0.75);
        assert_close(peak.outer_bloom.width, 5.0 * 0.75);

        let clamped = active_sector_style(ACTIVE_CYAN, 10.0, 1.0);
        assert_close(clamped.label_scale, 1.095);
        assert_close(clamped.tag_scale, ACTION_TAG_PEAK_SCALE);
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
                assert_eq!(style.tag_fill.a(), 255);
                assert_eq!(style.label_color, ACTIVE_LABEL_INK);
                assert!(
                    contrast_ratio(style.tag_fill, style.label_color) >= 4.5,
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
    fn target_path_anchors_do_not_overlap_when_target_is_named_scan_root() {
        for path in [r"C:\Users\Chip", r"\\server\share"] {
            let target = target_fixture("9.7 GB", "root", path, path);

            assert_eq!(
                target_path_anchors(&target, path),
                Some((path.to_owned(), String::new())),
                "root-equal target {path:?} must preserve the complete root only once"
            );
        }
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
        assert!(painter_clip.contains(draw_geometry.target_plate_rect().min));
        assert!(painter_clip.contains(draw_geometry.target_plate_rect().max));
        for index in 0..SECTORS.len() {
            assert!(painter_clip.contains(draw_geometry.action_center(index)));
            assert!(painter_clip.contains(draw_geometry.action_tag_rect(index).min));
            assert!(painter_clip.contains(draw_geometry.action_tag_rect(index).max));
        }
    }

    #[test]
    fn target_plate_geometry_is_exact_on_both_sides_at_both_scales() {
        const PLATE_WIDTH: f32 = 280.0;
        const PLATE_HEIGHT: f32 = 94.0;
        const PLATE_GAP: f32 = 12.0;
        const COMPOSITION_WIDTH: f32 = FAN_VISUAL_EXTENT + OUTER_RADIUS + PLATE_GAP + PLATE_WIDTH;
        const COMPOSITION_HEIGHT: f32 = OUTER_RADIUS * 2.0;

        for (work_area, requested_x, expected_orientation, expected_scale) in [
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)),
                240.0,
                Orientation::Right,
                1.0,
            ),
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)),
                1040.0,
                Orientation::Left,
                1.0,
            ),
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 300.0)),
                80.0,
                Orientation::Right,
                0.75,
            ),
            (
                Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 300.0)),
                420.0,
                Orientation::Left,
                0.75,
            ),
        ] {
            let geometry = TacticalArcGeometry::fit(
                Pos2::new(requested_x, work_area.center().y),
                work_area,
                1.0,
            )
            .expect("the complete target-plate composition fits");
            let plate = geometry.target_plate_rect();

            assert_eq!(geometry.orientation, expected_orientation);
            assert_close(geometry.scale, expected_scale);
            assert_close(plate.width(), PLATE_WIDTH * expected_scale);
            assert_close(plate.height(), PLATE_HEIGHT * expected_scale);
            assert_close(plate.center().y, geometry.center.y);
            assert!((geometry.bounds().width() - COMPOSITION_WIDTH * expected_scale).abs() < 0.001);
            assert_close(
                geometry.bounds().height(),
                COMPOSITION_HEIGHT * expected_scale,
            );
            assert!(work_area.contains(geometry.bounds().expand(SAFE_MARGIN).min));
            assert!(work_area.contains(geometry.bounds().expand(SAFE_MARGIN).max));

            match expected_orientation {
                Orientation::Right => assert_close(
                    geometry.center.x - OUTER_RADIUS * expected_scale - plate.right(),
                    PLATE_GAP * expected_scale,
                ),
                Orientation::Left => assert_close(
                    plate.left() - geometry.center.x - OUTER_RADIUS * expected_scale,
                    PLATE_GAP * expected_scale,
                ),
            }
        }
    }

    #[test]
    fn target_plate_fit_uses_only_the_approved_scale_floor() {
        let exact_compact_width =
            (FAN_VISUAL_EXTENT + OUTER_RADIUS + TARGET_PLATE_GAP + TARGET_PLATE_WIDTH)
                * COMPACT_SCALE
                + SAFE_MARGIN * 2.0;
        let exact_compact_height = OUTER_RADIUS * 2.0 * COMPACT_SCALE + SAFE_MARGIN * 2.0;
        let exact_compact = Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(exact_compact_width, exact_compact_height),
        );
        let compact = TacticalArcGeometry::fit(exact_compact.center(), exact_compact, 1.0)
            .expect("the exact compact composition plus safe margin fits");
        assert_close(compact.scale, 0.75);
        assert!(exact_compact.contains(compact.bounds().expand(SAFE_MARGIN).min));
        assert!(exact_compact.contains(compact.bounds().expand(SAFE_MARGIN).max));

        let too_narrow = Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(exact_compact_width - 0.1, exact_compact_height),
        );
        assert!(TacticalArcGeometry::fit(too_narrow.center(), too_narrow, 1.0).is_none());
        let too_short = Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(exact_compact_width, exact_compact_height - 0.1),
        );
        assert!(TacticalArcGeometry::fit(too_short.center(), too_short, 1.0).is_none());
    }

    #[test]
    fn target_plate_is_a_neutral_hit_while_sector_centers_keep_their_actions() {
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        for requested_x in [240.0, 1040.0] {
            let geometry = TacticalArcGeometry::fit(
                Pos2::new(requested_x, work_area.center().y),
                work_area,
                1.0,
            )
            .expect("test work area fits target plate");

            assert_eq!(
                geometry.hit_test(geometry.target_plate_rect().center()),
                None
            );
            for (index, sector) in SECTORS.iter().enumerate() {
                assert_eq!(
                    geometry.hit_test(geometry.action_center(index)),
                    Some(sector.action)
                );
            }
        }
    }

    #[test]
    fn target_plate_render_uses_the_approved_bands() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_max(Pos2::ZERO, Pos2::new(1280.0, 720.0));
        let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
        let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
            .expect("test work area fits target plate");
        let plate = arc.geometry.target_plate_rect();
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };
        let mut first_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        first_output.textures_delta.clear();
        let mut output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });

        let text = rendered_text_shapes(&output);
        let tag_shape = rendered_text(&text, "OBJECT / ACTION TARGET");
        let size_shape = rendered_text(&text, "9.7 GB");
        let name_shape = rendered_text(&text, "archive");
        let tag = logical_text_rect(tag_shape);
        let size = logical_text_rect(size_shape);
        let name = logical_text_rect(name_shape);
        let path = logical_text_rect(rendered_text(&text, r"C:\Data\archive"));
        let action = logical_text_rect(rendered_text(&text, "SELECT COMMAND"));
        assert_close(tag.left(), plate.left() + 12.0);
        assert_close(tag.center().y, plate.top() + 15.0);
        assert_close(size.left(), plate.left() + 12.0);
        assert_close(name.left(), size.right() + 8.0);
        assert_close(name.center().y, plate.top() + 36.0);
        assert_close(path.left(), plate.left() + 12.0);
        assert_close(path.center().y, plate.top() + 56.0);
        assert_close(action.left(), plate.left() + 12.0);
        assert_close(action.center().y, plate.top() + 79.0);
        assert_close(rendered_font_size(tag_shape), 8.0);
        assert_close(rendered_font_size(size_shape), 13.0);
        assert_close(rendered_font_size(name_shape), 20.0);
        assert!(rendered_font_size(name_shape) > rendered_font_size(size_shape));
        assert_eq!(rendered_color(tag_shape), hud::ORANGE);
        assert_eq!(rendered_color(size_shape), hud::ORANGE);
        assert_eq!(rendered_color(name_shape), Color32::WHITE);
        assert_close(rendered_baseline(size_shape), rendered_baseline(name_shape));

        fn has_separator(shape: &Shape, plate: Rect) -> bool {
            match shape {
                Shape::Vec(shapes) => shapes.iter().any(|shape| has_separator(shape, plate)),
                Shape::LineSegment { points, .. } => {
                    (points[0].x - (plate.left() + 12.0)).abs() <= EPSILON
                        && (points[1].x - (plate.right() - 12.0)).abs() <= EPSILON
                        && (points[0].y - (plate.top() + 68.0)).abs() <= EPSILON
                        && (points[1].y - (plate.top() + 68.0)).abs() <= EPSILON
                }
                _ => false,
            }
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|clipped| has_separator(&clipped.shape, plate)),
            "missing separator at y=68"
        );

        output.textures_delta.clear();
    }

    #[test]
    fn compact_left_target_plate_scales_every_internal_band() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 300.0));
        let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
        let mut arc = TacticalArcState::new(target, Pos2::new(420.0, 150.0), work_area, false)
            .expect("compact target plate fits");
        assert_eq!(arc.geometry.orientation, Orientation::Left);
        assert_close(arc.geometry.scale, COMPACT_SCALE);
        let plate = arc.geometry.target_plate_rect();
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };
        let mut first_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        first_output.textures_delta.clear();
        let mut output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let text = rendered_text_shapes(&output);
        let scale = COMPACT_SCALE;
        let tag_shape = rendered_text(&text, "OBJECT / ACTION TARGET");
        let size_shape = rendered_text(&text, "9.7 GB");
        let name_shape = rendered_text(&text, "archive");
        let tag = logical_text_rect(tag_shape);
        let size = logical_text_rect(size_shape);
        let name = logical_text_rect(name_shape);
        let path = logical_text_rect(rendered_text(&text, r"C:\Data\archive"));
        let action = logical_text_rect(rendered_text(&text, "SELECT COMMAND"));

        assert_close(tag.left(), plate.left() + TARGET_PLATE_INSET * scale);
        assert_close(
            tag.center().y,
            plate.top() + TARGET_PLATE_TAG_CENTER_Y * scale,
        );
        assert_close(size.left(), plate.left() + TARGET_PLATE_INSET * scale);
        assert_close(
            name.left(),
            size.right() + TARGET_PLATE_IDENTITY_GAP * scale,
        );
        assert_close(
            name.center().y,
            plate.top() + TARGET_PLATE_IDENTITY_CENTER_Y * scale,
        );
        assert_close(path.left(), plate.left() + TARGET_PLATE_INSET * scale);
        assert_close(
            path.center().y,
            plate.top() + TARGET_PLATE_PATH_CENTER_Y * scale,
        );
        assert_close(action.left(), plate.left() + TARGET_PLATE_INSET * scale);
        assert_close(
            action.center().y,
            plate.top() + TARGET_PLATE_ACTION_CENTER_Y * scale,
        );
        assert_close(rendered_font_size(tag_shape), 8.0 * scale);
        assert_close(rendered_font_size(size_shape), 13.0 * scale);
        assert_close(rendered_font_size(name_shape), 20.0 * scale);
        assert!(rendered_font_size(name_shape) > rendered_font_size(size_shape));
        assert_close(rendered_baseline(size_shape), rendered_baseline(name_shape));

        let expected_separator_y = plate.top() + TARGET_PLATE_SEPARATOR_Y * scale;
        let expected_separator_left = plate.left() + TARGET_PLATE_INSET * scale;
        let expected_separator_right = plate.right() - TARGET_PLATE_INSET * scale;
        let has_separator = output.shapes.iter().any(|clipped| {
            fn matches_separator(
                shape: &Shape,
                expected_left: f32,
                expected_right: f32,
                expected_y: f32,
            ) -> bool {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().any(|shape| {
                        matches_separator(shape, expected_left, expected_right, expected_y)
                    }),
                    Shape::LineSegment { points, .. } => {
                        (points[0].x - expected_left).abs() <= EPSILON
                            && (points[1].x - expected_right).abs() <= EPSILON
                            && (points[0].y - expected_y).abs() <= EPSILON
                            && (points[1].y - expected_y).abs() <= EPSILON
                    }
                    _ => false,
                }
            }
            matches_separator(
                &clipped.shape,
                expected_separator_left,
                expected_separator_right,
                expected_separator_y,
            )
        });
        assert!(has_separator);

        output.textures_delta.clear();
    }

    #[test]
    fn rendered_plate_uses_measured_grapheme_safe_name_and_path_fitting() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let full_name = "👨‍👩‍👧‍👦e\u{301}東京資料保管庫年度別監査記録バックアップ完全版";
        let full_path = concat!(
            r"C:\非常に長い保管場所\年度別\監査\バックアップ\追加資料\",
            r"過去年度\移行前\復旧候補\重複確認\原本照合\検証済み\最終報告.txt"
        );
        let target = target_fixture("184.0 GiB", full_name, full_path, r"C:\");
        let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
            .expect("full-scale target plate fits");
        let allocated_plate = arc.geometry.target_plate_rect();
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };
        let mut first_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        first_output.textures_delta.clear();
        let mut output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let text = rendered_text_shapes(&output);
        let size_shape = rendered_text(&text, "184.0 GiB");
        let fitted_name = text
            .iter()
            .map(|shape| shape.galley.text())
            .find(|label| label.ends_with('…') && !label.starts_with(r"C:\"))
            .expect("name uses an end ellipsis");
        let first_four = full_name.graphemes(true).take(4).collect::<String>();
        assert!(fitted_name.starts_with(&first_four));
        assert!(fitted_name.graphemes(true).count() >= 5);
        assert_ne!(fitted_name, full_name);
        let fitted_name_shape = rendered_text(&text, fitted_name);
        let content_width = allocated_plate.width() - TARGET_PLATE_INSET * 2.0 * arc.geometry.scale;
        let measured_name_width = content_width
            - size_shape.galley.size().x
            - TARGET_PLATE_IDENTITY_GAP * arc.geometry.scale;
        let expected_name = primitives::end_ellipsis(full_name, measured_name_width, 4, |value| {
            context.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(value.to_owned(), FontId::monospace(20.0), Color32::WHITE)
                    .size()
                    .x
            })
        })
        .expect("four leading name graphemes plus ellipsis fit after complete size reservation");
        assert_eq!(fitted_name, expected_name);
        assert_eq!(size_shape.galley.text(), "184.0 GiB");
        assert_close(
            logical_text_rect(fitted_name_shape).left(),
            logical_text_rect(size_shape).right() + TARGET_PLATE_IDENTITY_GAP * arc.geometry.scale,
        );

        let fitted_path = text
            .iter()
            .map(|shape| shape.galley.text())
            .find(|label| label.starts_with(r"C:\") && label.contains('…'))
            .expect("path uses a middle ellipsis");
        assert!(fitted_path.starts_with(r"C:\"));
        assert!(fitted_path.ends_with(r"\最終報告.txt"));
        assert_ne!(fitted_path, full_path);
        assert_close(allocated_plate.width(), TARGET_PLATE_WIDTH);
        assert_close(allocated_plate.height(), TARGET_PLATE_HEIGHT);

        output.textures_delta.clear();
    }

    #[test]
    fn target_plate_fails_closed_when_four_name_graphemes_cannot_fit() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let target = target_fixture(
            "1234567890123456789012345678901234567890",
            "WWWWWW",
            r"C:\Data\WWWWWW",
            r"C:\",
        );
        let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
            .expect("geometry itself fits");
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };

        let mut outcome = None;
        let mut output = context.run_ui(input, |ui| {
            outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
        });

        output.textures_delta.clear();
        assert_eq!(outcome, Some(TacticalArcOutcome::Dismiss));
    }

    #[test]
    fn named_directory_and_unc_scan_roots_render_with_full_tooltip_and_accessibility() {
        for (path, display_name) in [(r"C:\Users\Chip", "Chip"), (r"\\server\share", "share")] {
            let context = egui::Context::default();
            context.enable_accesskit();
            let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
            let target = target_fixture("9.7 GB", display_name, path, path);
            let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
                .expect("root target plate geometry fits");
            let input = egui::RawInput {
                screen_rect: Some(work_area),
                ..Default::default()
            };
            let mut first_outcome = None;
            let mut first_output = context.run_ui(input.clone(), |ui| {
                first_outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            let plate_node_id = first_output
                .platform_output
                .accesskit_update
                .as_ref()
                .and_then(|update| {
                    update.nodes.iter().find_map(|(id, node)| {
                        (node.role() == egui::accesskit::Role::Window
                            && node.label().is_some_and(|label| label.contains(path)))
                        .then_some(*id)
                    })
                });
            first_output.textures_delta.clear();
            assert_eq!(first_outcome, None, "root target {path:?} was dismissed");
            let plate_node_id = plate_node_id
                .expect("root plate exposes a stable Window node that can receive focus");

            let mut outcome = None;
            let focused_input = egui::RawInput {
                screen_rect: Some(work_area),
                events: vec![egui::Event::AccessKitActionRequest(
                    egui::accesskit::ActionRequest {
                        action: egui::accesskit::Action::Focus,
                        target_tree: egui::accesskit::TreeId::ROOT,
                        target_node: plate_node_id,
                        data: None,
                    },
                )],
                ..Default::default()
            };
            let mut focus_output = context.run_ui(focused_input, |ui| {
                outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            focus_output.textures_delta.clear();
            assert_eq!(outcome, None, "focused root target {path:?} was dismissed");

            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(work_area),
                    ..Default::default()
                },
                |ui| {
                    outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
                },
            );
            let rendered = rendered_text_shapes(&output)
                .iter()
                .map(|shape| shape.galley.text().to_owned())
                .collect::<Vec<_>>();
            let accessibility = output
                .platform_output
                .accesskit_update
                .as_ref()
                .and_then(|update| {
                    update.nodes.iter().map(|(_, node)| node).find(|node| {
                        node.role() == egui::accesskit::Role::Window
                            && node.label().is_some_and(|label| label.contains(path))
                    })
                })
                .map(|node| {
                    (
                        node.label()
                            .expect("matched root plate has an accessible description")
                            .to_owned(),
                        node.supports_action(egui::accesskit::Action::Focus),
                        node.supports_action(egui::accesskit::Action::Click),
                    )
                });
            output.textures_delta.clear();

            assert_eq!(outcome, None, "root target {path:?} was dismissed");
            let path_occurrences = rendered.iter().filter(|text| text.as_str() == path).count();
            assert!(
                path_occurrences >= 2,
                "expected full root {path:?} in both plate and focused tooltip, got {path_occurrences}"
            );
            let (description, supports_focus, supports_click) =
                accessibility.expect("root plate keeps a labeled Window accessibility node");
            for expected in ["9.7 GB", display_name, path, "SELECT COMMAND"] {
                assert!(
                    description.contains(expected),
                    "description omitted {expected:?}: {description:?}"
                );
            }
            assert!(supports_focus);
            assert!(!supports_click);
        }
    }

    #[test]
    fn target_plate_fit_cache_reuses_and_invalidates_by_font_and_geometry_scale() {
        let context = egui::Context::default();
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
        let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
            .expect("target plate fits");
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };

        for font in [FontId::monospace(9.0), FontId::monospace(9.0)] {
            let mut output = context.run_ui(input.clone(), |ui| {
                arc.show(ui.ctx(), font.clone());
            });
            output.textures_delta.clear();
        }
        assert_eq!(arc.target_plate_fit_count, 1, "same key must reuse fit");

        let mut base_size_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(37.0));
        });
        base_size_output.textures_delta.clear();
        assert_eq!(
            arc.target_plate_fit_count, 1,
            "base size change must reuse the identical actual scaled font set"
        );

        let mut font_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::proportional(9.0));
        });
        font_output.textures_delta.clear();
        assert_eq!(arc.target_plate_fit_count, 2, "font change must refit");

        arc.geometry.scale = COMPACT_SCALE;
        let mut scale_output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::proportional(9.0));
        });
        scale_output.textures_delta.clear();
        assert_eq!(
            arc.target_plate_fit_count, 3,
            "geometry scale change must refit"
        );
    }

    #[test]
    fn plate_accessibility_node_is_stable_focusable_and_untruncated() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let full_name = "archive-with-a-complete-untruncated-accessible-name";
        let full_path =
            r"C:\Data\archives\2026\deeply\nested\complete\untruncated\accessible\archive.bin";
        let target = target_fixture("184.0 GiB", full_name, full_path, r"C:\");
        let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
            .expect("target plate fits");
        let input = egui::RawInput {
            screen_rect: Some(work_area),
            ..Default::default()
        };

        let mut first_output = context.run_ui(input.clone(), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let (first_id, description, supports_focus, supports_click) = {
            let first_update = first_output
                .platform_output
                .accesskit_update
                .as_ref()
                .expect("accessibility tree generated");
            let (first_id, first_node) = first_update
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Window && node.label().is_some()
                })
                .expect("target plate owns a labeled Window accessibility node");
            (
                *first_id,
                first_node
                    .label()
                    .expect("plate has a description")
                    .to_owned(),
                first_node.supports_action(egui::accesskit::Action::Focus),
                first_node.supports_action(egui::accesskit::Action::Click),
            )
        };
        first_output.textures_delta.clear();
        for expected in ["184.0 GiB", full_name, full_path, "SELECT COMMAND"] {
            assert!(
                description.contains(expected),
                "description omitted {expected:?}: {description:?}"
            );
        }
        assert!(supports_focus);
        assert!(!supports_click);

        let mut second_output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let second_update = second_output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility tree remains active");
        let (second_id, second_node) = second_update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Window && node.label().is_some()
            })
            .expect("target plate accessibility node remains present");
        assert_eq!(*second_id, first_id);
        assert_eq!(second_node.label(), Some(description.as_str()));

        second_output.textures_delta.clear();
    }

    #[test]
    fn decorative_modal_shields_do_not_enter_keyboard_focus_order() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), false);

        let mut output = context.run_ui(raw_input(viewport, 1.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility tree generated");
        let focusable = update
            .nodes
            .iter()
            .filter(|(_, node)| node.supports_action(egui::accesskit::Action::Focus))
            .map(|(_, node)| (node.role(), node.label().map(str::to_owned)))
            .collect::<Vec<_>>();
        output.textures_delta.clear();

        assert_eq!(
            focusable
                .iter()
                .filter(|(role, _)| *role == egui::accesskit::Role::Button)
                .count(),
            SECTORS.len()
        );
        assert_eq!(
            focusable
                .iter()
                .filter(|(role, label)| {
                    *role == egui::accesskit::Role::Window
                        && label
                            .as_deref()
                            .is_some_and(|label| label.contains(r"C:\Data\archive"))
                })
                .count(),
            1
        );
        assert_eq!(
            focusable.len(),
            SECTORS.len() + 1,
            "only the sector buttons and target plate may receive keyboard focus: {focusable:?}"
        );
    }

    #[test]
    fn command_band_is_neutral_until_a_full_semantic_action_is_selected() {
        fn command_shape(
            pointer: Option<Pos2>,
        ) -> (String, Color32, Rect, Vec<egui::epaint::ClippedShape>) {
            let context = egui::Context::default();
            let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
            let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
            let mut arc = TacticalArcState::new(target, Pos2::new(320.0, 360.0), work_area, false)
                .expect("target plate fits");
            let plate = arc.geometry.target_plate_rect();
            let input = egui::RawInput {
                screen_rect: Some(work_area),
                ..Default::default()
            };
            let mut first_output = context.run_ui(input.clone(), |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            first_output.textures_delta.clear();
            let mut input = input;
            if let Some(pointer) = pointer {
                input.events.push(egui::Event::PointerMoved(pointer));
            }
            let mut output = context.run_ui(input, |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            let text = rendered_text_shapes(&output);
            let command = text
                .iter()
                .copied()
                .find(|shape| {
                    matches!(
                        shape.galley.text(),
                        "SELECT COMMAND"
                            | "OPEN IN EXPLORER"
                            | "MOVE TO RECYCLE BIN"
                            | "DELETE WITHOUT RECOVERY"
                    )
                })
                .expect("command band label rendered");
            let command_text = command.galley.text().to_owned();
            let command_color = command.galley.job.sections[0].format.color;
            let shapes = std::mem::take(&mut output.shapes);
            output.textures_delta.clear();
            (command_text, command_color, plate, shapes)
        }

        let (neutral_label, neutral_color, neutral_plate, neutral_shapes) = command_shape(None);
        assert_eq!(neutral_label, "SELECT COMMAND");
        assert_eq!(neutral_color, Color32::WHITE);
        fn has_plate_border(shape: &Shape, plate: Rect) -> bool {
            match shape {
                Shape::Vec(shapes) => shapes.iter().any(|shape| has_plate_border(shape, plate)),
                Shape::Rect(rect) => {
                    (rect.rect.width() - plate.width()).abs() <= EPSILON
                        && (rect.rect.height() - plate.height()).abs() <= EPSILON
                        && rect.stroke.color == TARGET_PLATE_BORDER
                }
                _ => false,
            }
        }
        assert!(
            neutral_shapes
                .iter()
                .any(|clipped| has_plate_border(&clipped.shape, neutral_plate))
        );

        let geometry = TacticalArcGeometry::fit(
            Pos2::new(320.0, 360.0),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)),
            1.0,
        )
        .expect("target plate fits");
        let (selected_label, selected_color, _, _) = command_shape(Some(
            geometry.action_center(TacticalAction::DeletePermanently.index()),
        ));
        assert_eq!(selected_label, "DELETE WITHOUT RECOVERY");
        assert_eq!(selected_color, ACTIVE_MAGENTA);
    }

    #[test]
    fn first_frame_primary_outside_dismisses_and_sector_click_executes_before_paint() {
        fn click_result(use_outside: bool) -> (Option<TacticalArcOutcome>, egui::FullOutput, Rect) {
            let context = egui::Context::default();
            let work_area = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
            let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
            let mut arc = TacticalArcState::new_with_motion_query(
                target,
                Pos2::new(320.0, 360.0),
                work_area,
                false,
                || Some(true),
            )
            .expect("target plate fits");
            let pointer = if use_outside {
                Pos2::new(1200.0, 40.0)
            } else {
                arc.geometry.action_center(TacticalAction::Recycle.index())
            };
            let mut input = egui::RawInput {
                screen_rect: Some(work_area),
                ..Default::default()
            };
            input.events.extend([
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            let mut outcome = None;
            let mut output = context.run_ui(input, |ui| {
                outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            output.textures_delta.clear();
            (outcome, output, work_area)
        }

        let (outside_outcome, outside_output, viewport) = click_result(true);
        assert_eq!(outside_outcome, Some(TacticalArcOutcome::Dismiss));
        assert!(!leaf_shapes(&outside_output).iter().any(
            |shape| matches!(shape, Shape::Rect(rect) if rect.fill == MODAL_SCRIM && rect.rect == viewport)
        ));

        let (sector_outcome, sector_output, viewport) = click_result(false);
        assert_eq!(
            sector_outcome,
            Some(TacticalArcOutcome::Action(TacticalAction::Recycle))
        );
        assert!(!leaf_shapes(&sector_output).iter().any(
            |shape| matches!(shape, Shape::Rect(rect) if rect.fill == MODAL_SCRIM && rect.rect == viewport)
        ));
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
        assert_eq!(visible_labels, 10);
    }

    fn collect_leaf_shapes<'a>(shape: &'a Shape, found: &mut Vec<&'a Shape>) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_leaf_shapes(shape, found);
                }
            }
            shape => found.push(shape),
        }
    }

    fn leaf_shapes(output: &egui::FullOutput) -> Vec<&Shape> {
        let mut found = Vec::new();
        for clipped in &output.shapes {
            collect_leaf_shapes(&clipped.shape, &mut found);
        }
        found
    }

    fn raw_input(viewport: Rect, time: f64, pointer: Option<Pos2>) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(viewport),
            time: Some(time),
            predicted_dt: 0.0,
            ..Default::default()
        };
        if let Some(pointer) = pointer {
            input.events.push(egui::Event::PointerMoved(pointer));
        }
        input
    }

    fn pointer_click_input(
        viewport: Rect,
        time: f64,
        pointer: Pos2,
        button: egui::PointerButton,
    ) -> egui::RawInput {
        let mut input = raw_input(viewport, time, Some(pointer));
        input.events.extend([
            egui::Event::PointerButton {
                pos: pointer,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: pointer,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        input
    }

    fn key_input(
        viewport: Rect,
        time: f64,
        key: Key,
        modifiers: egui::Modifiers,
    ) -> egui::RawInput {
        let mut input = raw_input(viewport, time, None);
        input.events.push(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        });
        input
    }

    fn has_viewport_scrim(output: &egui::FullOutput, viewport: Rect) -> bool {
        leaf_shapes(output).iter().any(
            |shape| matches!(shape, Shape::Rect(rect) if rect.fill == MODAL_SCRIM && rect.rect == viewport),
        )
    }

    #[test]
    fn repeated_opening_secondary_is_ignored_until_a_neutral_frame_then_next_click_dismisses() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), false);
        let pointer = Pos2::new(1200.0, 40.0);

        let mut opening_outcome = None;
        let mut opening_output = context.run_ui(
            pointer_click_input(viewport, 1.0, pointer, egui::PointerButton::Secondary),
            |ui| {
                opening_outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            },
        );
        assert_eq!(opening_outcome, None);
        assert!(has_viewport_scrim(&opening_output, viewport));
        opening_output.textures_delta.clear();

        let mut repeated_opening_outcome = None;
        let mut repeated_opening_output = context.run_ui(
            pointer_click_input(viewport, 1.0, pointer, egui::PointerButton::Secondary),
            |ui| {
                repeated_opening_outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            },
        );
        assert_eq!(repeated_opening_outcome, None);
        assert!(has_viewport_scrim(&repeated_opening_output, viewport));
        repeated_opening_output.textures_delta.clear();

        let mut neutral_outcome = None;
        let mut neutral_output = context.run_ui(raw_input(viewport, 1.5, Some(pointer)), |ui| {
            neutral_outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        assert_eq!(neutral_outcome, None);
        assert!(has_viewport_scrim(&neutral_output, viewport));
        neutral_output.textures_delta.clear();

        let mut closing_outcome = None;
        let mut closing_output = context.run_ui(
            pointer_click_input(viewport, 2.0, pointer, egui::PointerButton::Secondary),
            |ui| {
                closing_outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            },
        );
        closing_output.textures_delta.clear();
        assert_eq!(closing_outcome, Some(TacticalArcOutcome::Dismiss));
        assert!(!has_viewport_scrim(&closing_output, viewport));
    }

    #[test]
    fn keyboard_navigation_updates_visual_selection_before_first_paint() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), true);
        let mut input = raw_input(viewport, 1.0, None);
        input.events.push(egui::Event::Key {
            key: Key::ArrowRight,
            physical_key: Some(Key::ArrowRight),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });

        let mut output = context.run_ui(input, |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let text = rendered_text_shapes(&output);
        let active_label = rendered_text(&text, "2  BIN");
        let active_color = rendered_color(active_label);
        let command_matches = text
            .iter()
            .any(|shape| shape.galley.text() == "MOVE TO RECYCLE BIN");
        output.textures_delta.clear();
        assert_eq!(active_color, ACTIVE_LABEL_INK);
        assert!(command_matches);
    }

    #[test]
    fn exactly_one_alpha_184_scrim_covers_each_full_content_viewport() {
        let scrim = Color32::from_rgba_unmultiplied(0, 0, 0, 184);
        for viewport in [
            Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0)),
            Rect::from_min_size(Pos2::new(17.0, 29.0), Vec2::new(1280.0, 720.0)),
        ] {
            let context = egui::Context::default();
            let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
            let mut arc = TacticalArcState::new_with_motion_query(
                target,
                viewport.center(),
                viewport,
                false,
                || Some(false),
            )
            .expect("test viewport fits tactical arc");
            let mut content_rect = Rect::NOTHING;
            let mut output = context.run_ui(raw_input(viewport, 1.0, None), |ui| {
                content_rect = ui.ctx().content_rect();
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });

            let matching = leaf_shapes(&output)
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == scrim => Some(rect.rect),
                    _ => None,
                })
                .collect::<Vec<_>>();
            output.textures_delta.clear();
            assert_eq!(matching, vec![content_rect]);
            assert_eq!(content_rect, viewport);
        }
    }

    #[test]
    fn modal_shapes_follow_base_scrim_tether_fan_hub_plate_order_and_stay_contained() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
        let mut arc = TacticalArcState::new_with_motion_query(
            target,
            Pos2::new(24.0, 360.0),
            viewport,
            false,
            || Some(false),
        )
        .expect("test viewport fits tactical arc");
        assert!(arc.geometry.origin.distance(arc.geometry.center) > 2.0);
        let plate = arc.geometry.target_plate_rect();
        let origin = arc.geometry.origin;
        let center = arc.geometry.center;
        let base_color = Color32::from_rgb(0x51, 0x52, 0x53);
        let scrim = Color32::from_rgba_unmultiplied(0, 0, 0, 184);
        let mut warm_up = context.run_ui(raw_input(viewport, 0.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        warm_up.textures_delta.clear();
        let mut output = context.run_ui(raw_input(viewport, 1.0, None), |ui| {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(2.0, 2.0), Vec2::splat(4.0)),
                0.0,
                base_color,
            );
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let shapes = leaf_shapes(&output);
        let index_of = |predicate: &dyn Fn(&Shape) -> bool| {
            shapes
                .iter()
                .position(|shape| predicate(shape))
                .expect("expected rendered shape")
        };
        let base_index =
            index_of(&|shape| matches!(shape, Shape::Rect(rect) if rect.fill == base_color));
        let scrim_index = index_of(
            &|shape| matches!(shape, Shape::Rect(rect) if rect.fill == scrim && rect.rect == viewport),
        );
        let tether_index = index_of(&|shape| {
            matches!(shape, Shape::LineSegment { points, stroke }
                if *points == [origin, center] && stroke.color == hud::ORANGE)
        });
        let fan_index = index_of(&|shape| matches!(shape, Shape::Mesh(_)));
        let hub_index = index_of(&|shape| {
            matches!(shape, Shape::Circle(circle)
                if circle.center == center && circle.fill == Color32::from_rgb(7, 9, 10))
        });
        let plate_index = index_of(&|shape| {
            matches!(shape, Shape::Rect(rect)
                if rect.rect == plate && rect.fill == TARGET_PLATE_BACKGROUND)
        });
        assert!(base_index < scrim_index);
        assert!(scrim_index < tether_index);
        assert!(tether_index < fan_index);
        assert!(fan_index < hub_index);
        assert!(hub_index < plate_index);

        let scrim_clipped_index = output
            .shapes
            .iter()
            .position(|clipped| {
                let mut leaves = Vec::new();
                collect_leaf_shapes(&clipped.shape, &mut leaves);
                leaves.iter().any(|shape| {
                    matches!(shape, Shape::Rect(rect) if rect.fill == scrim && rect.rect == viewport)
                })
            })
            .expect("scrim clipped shape");
        for clipped in &output.shapes[scrim_clipped_index..] {
            assert_eq!(clipped.clip_rect, viewport);
            assert!(viewport.contains(clipped.shape.visual_bounding_rect().min));
            assert!(viewport.contains(clipped.shape.visual_bounding_rect().max));
        }
        output.textures_delta.clear();
    }

    #[test]
    fn disabled_opening_underlay_and_later_viewport_shield_block_click_through() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let target = target_fixture("9.7 GB", "archive", r"C:\Data\archive", r"C:\");
        let mut arc = TacticalArcState::new_with_motion_query(
            target,
            Pos2::new(320.0, 360.0),
            viewport,
            false,
            || Some(false),
        )
        .expect("test viewport fits tactical arc");
        let outside = Pos2::new(1200.0, 40.0);
        assert!(!arc.geometry.bounds().contains(outside));
        let button_rect = Rect::from_center_size(outside, Vec2::splat(72.0));
        let underlay_clicked = Cell::new(false);
        let mut run_frame = |input: egui::RawInput, underlay_enabled: bool| {
            let mut outcome = None;
            let mut output = context.run_ui(input, |ui| {
                let response = ui
                    .add_enabled_ui(underlay_enabled, |ui| {
                        ui.put(button_rect, egui::Button::new("UNDERLAY"))
                    })
                    .inner;
                underlay_clicked.set(underlay_clicked.get() || response.clicked());
                outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            output.textures_delta.clear();
            outcome
        };
        assert_eq!(
            run_frame(
                pointer_click_input(viewport, 1.0, outside, egui::PointerButton::Secondary,),
                false,
            ),
            None,
            "the app disables its underlay while the opening secondary event is ignored"
        );
        assert!(!underlay_clicked.get());

        assert_eq!(
            run_frame(
                pointer_click_input(viewport, 2.0, outside, egui::PointerButton::Primary,),
                true,
            ),
            Some(TacticalArcOutcome::Dismiss)
        );
        assert!(!underlay_clicked.get());
    }

    #[test]
    fn pointer_hover_renders_saturated_pulsing_label_and_layered_bounded_glow() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(true), false);
        let pointer = arc.geometry.action_center(TacticalAction::Recycle.index());
        let mut warm_up = context.run_ui(raw_input(viewport, 9.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        warm_up.textures_delta.clear();
        let mut baseline = context.run_ui(raw_input(viewport, 10.0, Some(pointer)), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let baseline_text = rendered_text_shapes(&baseline);
        let baseline_label = rendered_text(&baseline_text, "2  BIN");
        assert_close(rendered_font_size(baseline_label), ACTION_TAG_FONT_POINTS);
        assert_eq!(rendered_color(baseline_label), ACTIVE_LABEL_INK);
        let baseline_label_center = logical_text_rect(baseline_label).center();
        assert!(
            baseline_label_center.distance(arc.geometry.center) > OUTER_RADIUS * arc.geometry.scale,
            "the approved Reactor Beat label must sit on an outer command tag"
        );
        let baseline_shapes = leaf_shapes(&baseline);
        let baseline_shadow = baseline_shapes.iter().find_map(|shape| match shape {
            Shape::Rect(rect)
                if rect.fill == ACTIVE_LIME
                    && rect.blur_width == ACTION_TAG_SHADOW_BLUR
                    && rect.rect.contains(baseline_label_center) =>
            {
                Some(rect.rect)
            }
            _ => None,
        });
        let baseline_tag = baseline_shapes.iter().find_map(|shape| match shape {
            Shape::Rect(rect)
                if rect.fill == ACTIVE_LIME
                    && rect.blur_width == 0.0
                    && rect.rect.contains(baseline_label_center) =>
            {
                Some(rect.rect)
            }
            _ => None,
        });
        baseline.textures_delta.clear();
        baseline_shadow.expect("Reactor Beat keeps the preview's 22 px sector-colored box-shadow");
        let baseline_tag = baseline_tag
            .expect("Reactor Beat starts with the preview's opaque semantic action tag");
        assert_close(baseline_tag.width(), ACTION_TAG_WIDTH);
        assert_close(baseline_tag.height(), ACTION_TAG_HEIGHT);

        let peak_time = 10.0 + f64::from(0.12 * REACTOR_BEAT_SECONDS);
        let mut settle = context.run_ui(raw_input(viewport, peak_time, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        settle.textures_delta.clear();
        let mut peak = context.run_ui(raw_input(viewport, peak_time, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let peak_text = rendered_text_shapes(&peak);
        let peak_label = rendered_text(&peak_text, "2  BIN");
        assert_close(
            rendered_font_size(peak_label),
            ACTION_TAG_FONT_POINTS * 1.095,
        );
        assert_eq!(rendered_color(peak_label), ACTIVE_LABEL_INK);

        let shapes = leaf_shapes(&peak);
        let peak_shadow = shapes
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(rect)
                    if rect.fill == Color32::from_rgb(0xff, 0xff, 0x54)
                        && rect.blur_width == ACTION_TAG_SHADOW_BLUR
                        && rect.rect.contains(logical_text_rect(peak_label).center()) =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("the Reactor Beat shadow brightens with the whole action tag");
        let peak_tag = shapes
            .iter()
            .find_map(|shape| match shape {
                Shape::Rect(rect)
                    if rect.fill == Color32::from_rgb(0xff, 0xff, 0x54)
                        && rect.blur_width == 0.0
                        && rect.rect.contains(logical_text_rect(peak_label).center()) =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("the whole action tag brightens at the Reactor Beat peak");
        assert_eq!(peak_shadow, peak_tag);
        assert!((peak_tag.width() - ACTION_TAG_WIDTH * ACTION_TAG_PEAK_SCALE).abs() < 0.001);
        assert!((peak_tag.height() - ACTION_TAG_HEIGHT * ACTION_TAG_PEAK_SCALE).abs() < 0.001);
        let active_mesh = shapes
            .iter()
            .find_map(|shape| match shape {
                Shape::Mesh(mesh)
                    if mesh
                        .vertices
                        .iter()
                        .all(|vertex| vertex.color == ACTIVE_LIME) =>
                {
                    Some(mesh)
                }
                _ => None,
            })
            .expect("active sector keeps its exact opaque semantic fill");
        assert!(
            active_mesh
                .vertices
                .iter()
                .all(|vertex| vertex.color.to_array() == [0xbd, 0xff, 0x3e, 0xff])
        );
        let active_stroke_widths = shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path)
                    if path
                        .points
                        .iter()
                        .any(|point| point.distance(pointer) < OUTER_RADIUS) =>
                {
                    Some(path.stroke.width)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(active_stroke_widths.contains(&2.0));
        assert!(active_stroke_widths.contains(&5.0));
        assert!(active_stroke_widths.iter().all(|width| *width <= 5.0));

        let repaint_delay = peak
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output")
            .repaint_delay;
        peak.textures_delta.clear();
        assert_eq!(repaint_delay, Duration::from_millis(16));
    }

    #[test]
    fn source_target_lock_is_quiet_while_sector_buttons_own_the_reactor_beat() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let source_rect = Rect::from_min_max(Pos2::new(36.0, 72.0), Pos2::new(196.0, 232.0));
        let target = target_fixture("53.2 GiB", "Users", r"C:\Users", r"C:\");
        let mut arc = TacticalArcState::new_with_source_rect_and_motion_query(
            target,
            Pos2::new(640.0, 360.0),
            viewport,
            false,
            source_rect,
            || Some(true),
        )
        .expect("target lock composition fits");

        let mut warm_up = context.run_ui(raw_input(viewport, 10.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        warm_up.textures_delta.clear();
        let mut settle = context.run_ui(raw_input(viewport, 10.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        settle.textures_delta.clear();
        let mut output = context.run_ui(
            raw_input(
                viewport,
                10.0 + f64::from(0.12 * REACTOR_BEAT_SECONDS),
                None,
            ),
            |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            },
        );

        let text = rendered_text_shapes(&output);
        assert!(
            text.iter()
                .all(|shape| shape.galley.text() != "TARGET LOCKED")
        );

        fn has_quiet_target_lock(shape: &Shape, source_rect: Rect) -> bool {
            match shape {
                Shape::Vec(shapes) => shapes
                    .iter()
                    .any(|shape| has_quiet_target_lock(shape, source_rect)),
                Shape::Rect(rect) => {
                    rect.rect == source_rect
                        && rect.fill == Color32::TRANSPARENT
                        && rect.stroke.color
                            == Color32::from_rgba_unmultiplied(0xff, 0x5a, 0x22, 176)
                        && rect.stroke.width <= 1.0
                }
                _ => false,
            }
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|clipped| has_quiet_target_lock(&clipped.shape, source_rect))
        );
        assert_eq!(
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .expect("root viewport output")
                .repaint_delay,
            Duration::MAX
        );
        output.textures_delta.clear();
    }

    #[test]
    fn reduced_motion_and_keyboard_only_are_static_without_autonomous_repaint() {
        for query in [Some(false), None] {
            let context = egui::Context::default();
            let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
            let mut arc = arc_with_motion_query(|| query, false);
            let pointer = arc.geometry.action_center(TacticalAction::Recycle.index());
            let mut first = context.run_ui(raw_input(viewport, 10.0, Some(pointer)), |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            first.textures_delta.clear();
            let mut settle = context.run_ui(raw_input(viewport, 20.0, None), |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            settle.textures_delta.clear();
            let mut output = context.run_ui(raw_input(viewport, 20.0, None), |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            let text = rendered_text_shapes(&output);
            let label = rendered_text(&text, "2  BIN");
            assert_close(rendered_font_size(label), ACTION_TAG_FONT_POINTS);
            assert_eq!(rendered_color(label), ACTIVE_LABEL_INK);
            assert_eq!(
                output
                    .viewport_output
                    .get(&egui::ViewportId::ROOT)
                    .expect("root viewport output")
                    .repaint_delay,
                Duration::MAX
            );
            output.textures_delta.clear();
        }

        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(true), true);
        let mut warm_up = context.run_ui(raw_input(viewport, 9.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        warm_up.textures_delta.clear();
        let mut settle = context.run_ui(raw_input(viewport, 10.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        settle.textures_delta.clear();
        let mut output = context.run_ui(raw_input(viewport, 10.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let text = rendered_text_shapes(&output);
        let label = rendered_text(&text, "1  OPEN");
        assert_close(rendered_font_size(label), ACTION_TAG_FONT_POINTS);
        assert_eq!(rendered_color(label), ACTIVE_LABEL_INK);
        assert_eq!(
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .expect("root viewport output")
                .repaint_delay,
            Duration::MAX
        );
        output.textures_delta.clear();
    }

    #[test]
    fn enter_uses_keyboard_target_while_pointer_owns_visual_selection() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(true), true);
        let pointer = arc.geometry.action_center(TacticalAction::Recycle.index());
        let mut opening_output = context.run_ui(raw_input(viewport, 9.0, None), |ui| {
            assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
        });
        opening_output.textures_delta.clear();
        let mut settle_output = context.run_ui(raw_input(viewport, 9.5, None), |ui| {
            assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
        });
        settle_output.textures_delta.clear();
        let mut input = raw_input(viewport, 10.0, Some(pointer));
        input.events.push(egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut outcome = None;
        let mut output = context.run_ui(input, |ui| {
            outcome = arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let visual_action = arc.visual_active_action();
        let has_scrim = has_viewport_scrim(&output, viewport);
        output.textures_delta.clear();
        let mut cause_output = context.run_ui(raw_input(viewport, 11.0, None), |_ui| {});
        let repaint_causes = context.repaint_causes();
        cause_output.textures_delta.clear();
        assert_eq!(visual_action, Some(TacticalAction::Recycle));
        assert_eq!(
            outcome,
            Some(TacticalArcOutcome::Action(TacticalAction::OpenInExplorer))
        );
        assert!(!has_scrim);
        assert!(
            repaint_causes.iter().all(|cause| cause.file != file!()),
            "closing the arc must not request its 16 ms animation repaint: {repaint_causes:?}"
        );
    }

    #[test]
    fn plate_accessibility_identity_and_description_ignore_pulse_frames() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(true), false);
        let pointer = arc
            .geometry
            .action_center(TacticalAction::DeletePermanently.index());
        let plate_node = |output: &egui::FullOutput| {
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .expect("accessibility update")
                .nodes
                .iter()
                .find(|(_, node)| {
                    node.role() == egui::accesskit::Role::Window && node.label().is_some()
                })
                .map(|(id, node)| (*id, node.label().expect("plate description").to_owned()))
                .expect("plate Window node")
        };
        let mut neutral = context.run_ui(raw_input(viewport, 9.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let neutral_node = plate_node(&neutral);
        neutral.textures_delta.clear();
        let mut baseline = context.run_ui(raw_input(viewport, 10.0, Some(pointer)), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        let baseline_node = plate_node(&baseline);
        baseline.textures_delta.clear();
        let mut peak = context.run_ui(
            raw_input(
                viewport,
                10.0 + f64::from(0.12 * REACTOR_BEAT_SECONDS),
                None,
            ),
            |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            },
        );
        let peak_node = plate_node(&peak);
        peak.textures_delta.clear();
        assert_eq!(neutral_node.0, baseline_node.0);
        assert!(neutral_node.1.contains("SELECT COMMAND"));
        assert_ne!(neutral_node.1, baseline_node.1);
        assert_eq!(peak_node, baseline_node);
        assert!(peak_node.1.contains("DELETE WITHOUT RECOVERY"));
        assert!(!peak_node.1.contains("1.095"));
        assert!(!peak_node.1.contains("pulse"));
    }

    #[test]
    fn keyboard_open_enters_modal_focus_and_tab_wraps_across_four_controls() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), true);
        let action_id = |index| Id::new("tactical-action").with(index);
        let plate_id = target_plate_response_id(&arc.target);
        let origin_id = arc.target.origin_focus;
        context.memory_mut(|memory| memory.request_focus(origin_id));

        let mut opening = context.run_ui(raw_input(viewport, 1.0, None), |ui| {
            assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
        });
        opening.textures_delta.clear();
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(action_id(0))
        );
        assert_ne!(context.memory(|memory| memory.focused()), Some(origin_id));
        assert!(
            !arc.focus_request_pending,
            "initial focus request must be one-shot"
        );

        for (frame, expected) in [
            (2.0, action_id(1)),
            (3.0, action_id(2)),
            (4.0, plate_id),
            (5.0, action_id(0)),
        ] {
            let mut output = context.run_ui(
                key_input(viewport, frame, Key::Tab, egui::Modifiers::NONE),
                |ui| {
                    assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
                },
            );
            output.textures_delta.clear();
            assert_eq!(context.memory(|memory| memory.focused()), Some(expected));
        }

        let mut backward = context.run_ui(
            key_input(viewport, 6.0, Key::Tab, egui::Modifiers::SHIFT),
            |ui| {
                assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
            },
        );
        backward.textures_delta.clear();
        assert_eq!(context.memory(|memory| memory.focused()), Some(plate_id));
    }

    #[test]
    fn pointer_open_first_tab_focuses_explorer_and_enter_executes_it() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), false);
        let explorer_id = Id::new("tactical-action").with(0);

        let mut tab = context.run_ui(
            key_input(viewport, 1.0, Key::Tab, egui::Modifiers::NONE),
            |ui| assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None),
        );
        let accesskit_focus = tab
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility update")
            .focus;
        tab.textures_delta.clear();
        assert_eq!(arc.focus_slot, Some(ModalFocusSlot::Action(0)));
        assert_eq!(arc.keyboard_index, Some(0));
        assert_eq!(context.memory(|memory| memory.focused()), Some(explorer_id));
        assert_eq!(accesskit_focus, explorer_id.accesskit_id());

        let mut outcome = None;
        let mut enter = context.run_ui(
            key_input(viewport, 2.0, Key::Enter, egui::Modifiers::NONE),
            |ui| outcome = arc.show(ui.ctx(), FontId::monospace(9.0)),
        );
        enter.textures_delta.clear();
        assert_eq!(
            outcome,
            Some(TacticalArcOutcome::Action(TacticalAction::OpenInExplorer))
        );
    }

    #[test]
    fn pointer_open_first_shift_tab_focuses_plate_without_selecting_an_action() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), false);
        let plate_id = target_plate_response_id(&arc.target);

        let mut shift_tab = context.run_ui(
            key_input(viewport, 1.0, Key::Tab, egui::Modifiers::SHIFT),
            |ui| assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None),
        );
        let accesskit_focus = shift_tab
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accessibility update")
            .focus;
        shift_tab.textures_delta.clear();
        assert_eq!(arc.focus_slot, Some(ModalFocusSlot::Plate));
        assert_eq!(arc.keyboard_index, None);
        assert_eq!(context.memory(|memory| memory.focused()), Some(plate_id));
        assert_eq!(accesskit_focus, plate_id.accesskit_id());

        let mut outcome = None;
        let mut enter = context.run_ui(
            key_input(viewport, 2.0, Key::Enter, egui::Modifiers::NONE),
            |ui| outcome = arc.show(ui.ctx(), FontId::monospace(9.0)),
        );
        enter.textures_delta.clear();
        assert_eq!(outcome, None);
    }

    #[test]
    fn focused_plate_tooltip_keeps_full_path_and_enter_uses_last_logical_action() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(false), true);
        let full_path = arc.target.path.display().to_string();

        let mut opening = context.run_ui(raw_input(viewport, 1.0, None), |ui| {
            arc.show(ui.ctx(), FontId::monospace(9.0));
        });
        opening.textures_delta.clear();
        for frame in 2..=4 {
            let mut output = context.run_ui(
                key_input(viewport, f64::from(frame), Key::Tab, egui::Modifiers::NONE),
                |ui| {
                    assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
                },
            );
            output.textures_delta.clear();
        }

        let mut tooltip = context.run_ui(raw_input(viewport, 4.5, None), |ui| {
            assert_eq!(arc.show(ui.ctx(), FontId::monospace(9.0)), None);
        });
        let occurrences = rendered_text_shapes(&tooltip)
            .iter()
            .filter(|shape| shape.galley.text() == full_path)
            .count();
        tooltip.textures_delta.clear();
        assert!(
            occurrences >= 2,
            "focused plate must expose the full-path tooltip"
        );

        let mut outcome = None;
        let mut enter = context.run_ui(
            key_input(viewport, 5.0, Key::Enter, egui::Modifiers::NONE),
            |ui| outcome = arc.show(ui.ctx(), FontId::monospace(9.0)),
        );
        enter.textures_delta.clear();
        assert_eq!(
            outcome,
            Some(TacticalArcOutcome::Action(
                TacticalAction::DeletePermanently
            ))
        );
    }

    #[test]
    fn full_reactor_cycle_uses_at_most_nine_rendered_label_font_sizes() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0));
        let mut arc = arc_with_motion_query(|| Some(true), false);
        let pointer = arc.geometry.action_center(TacticalAction::Recycle.index());
        let mut sizes = Vec::<f32>::new();

        for sample in 0..=1024 {
            let phase = sample as f32 / 1024.0;
            let time = 10.0 + f64::from(phase * REACTOR_BEAT_SECONDS);
            let mut output = context.run_ui(raw_input(viewport, time, Some(pointer)), |ui| {
                arc.show(ui.ctx(), FontId::monospace(9.0));
            });
            let text = rendered_text_shapes(&output);
            let size = rendered_font_size(rendered_text(&text, "2  BIN"));
            if !sizes.contains(&size) {
                sizes.push(size);
            }
            output.textures_delta.clear();
        }

        assert!(
            sizes.len() <= 9,
            "unbounded rendered font variants: {sizes:?}"
        );
        assert!(sizes.contains(&ACTION_TAG_FONT_POINTS));
        assert!(sizes.contains(&(ACTION_TAG_FONT_POINTS * 1.095)));
    }
}
