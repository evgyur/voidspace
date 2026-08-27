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
        color: hud::CYAN,
    },
    SectorSpec {
        action: TacticalAction::Recycle,
        short: "BIN",
        label: "MOVE TO RECYCLE BIN",
        accessible_name: "Move to Recycle Bin",
        shortcut: "2",
        color: hud::LIME,
    },
    SectorSpec {
        action: TacticalAction::DeletePermanently,
        short: "VOID",
        label: "DELETE WITHOUT RECOVERY",
        accessible_name: "Delete without recovery",
        shortcut: "3",
        color: hud::MAGENTA,
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
                let offset = area_rect.min.to_vec2();
                let center = geometry.center - offset;
                let local_geometry = TacticalArcGeometry {
                    center,
                    origin: geometry.origin - offset,
                    work_area: geometry.work_area.translate(-offset),
                    ..geometry
                };
                let (rect, response) = ui.allocate_exact_size(area_rect.size(), Sense::click());
                let painter = ui.painter_at(rect);
                let pointer = ui.ctx().pointer_hover_pos();
                let hovered = pointer.and_then(|position| geometry.hit_test(position));

                for (index, sector) in SECTORS.iter().enumerate() {
                    let active = hovered == Some(sector.action)
                        || (hovered.is_none() && self.keyboard_index == Some(index));
                    let angle = local_geometry.action_angle(index);
                    let mut points = vec![center];
                    for step in 0..=12 {
                        let fraction = step as f32 / 12.0;
                        let sample = angle - SECTOR_HALF_ANGLE + SECTOR_HALF_ANGLE * 2.0 * fraction;
                        points.push(
                            center + Vec2::angled(sample) * (OUTER_RADIUS * local_geometry.scale),
                        );
                    }
                    painter.add(Shape::convex_polygon(
                        points,
                        if active {
                            Color32::from_rgba_unmultiplied(
                                sector.color.r(),
                                sector.color.g(),
                                sector.color.b(),
                                44,
                            )
                        } else {
                            Color32::from_rgba_unmultiplied(11, 14, 16, 245)
                        },
                        Stroke::new(if active { 2.0 } else { 1.0 }, sector.color),
                    ));
                    let action_center = local_geometry.action_center(index);
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
                            Vec2::splat(56.0 * local_geometry.scale),
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
                    HUB_RADIUS * local_geometry.scale,
                    Color32::from_rgb(7, 9, 10),
                );
                painter.circle_stroke(
                    center,
                    HUB_RADIUS * local_geometry.scale,
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
                    painter.line_segment([center, pointer - offset], Stroke::new(1.5, hud::CYAN));
                }
                let rail = local_geometry.rail_rect();
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
    use super::*;

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
}
