use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Id, Key, Pos2, Rect, Sense, Stroke, Vec2};
use voidspace_model::{NodeId, ScanId};

use crate::hud;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextTarget {
    pub scan_id: ScanId,
    pub generation: u64,
    pub node_id: NodeId,
    pub path: PathBuf,
    pub is_directory: bool,
    pub root: PathBuf,
    pub view_root: NodeId,
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

impl TacticalAction {
    pub const ALL: [Self; 3] = [Self::OpenInExplorer, Self::Recycle, Self::DeletePermanently];

    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenInExplorer => "OPEN / EXPLORER",
            Self::Recycle => "MOVE / RECYCLE",
            Self::DeletePermanently => "DELETE / FOREVER",
        }
    }

    pub const fn shortcut(self) -> &'static str {
        match self {
            Self::OpenInExplorer => "1",
            Self::Recycle => "2",
            Self::DeletePermanently => "3",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TacticalArcGeometry {
    pub center: Pos2,
    inner_radius: f32,
    outer_radius: f32,
}

impl TacticalArcGeometry {
    pub fn new(center: Pos2, inner_radius: f32, outer_radius: f32) -> Self {
        Self {
            center,
            inner_radius,
            outer_radius,
        }
    }

    pub fn clamped(requested: Pos2, area: Rect, outer_radius: f32) -> Self {
        let margin = outer_radius + 8.0;
        let center = Pos2::new(
            requested
                .x
                .clamp(area.left() + margin, area.right() - margin),
            requested
                .y
                .clamp(area.top() + margin, area.bottom() - margin),
        );
        Self::new(center, 36.0, outer_radius)
    }

    pub fn bounds(self) -> Rect {
        Rect::from_center_size(self.center, Vec2::splat(self.outer_radius * 2.0))
    }

    pub fn action_centers(self) -> [(TacticalAction, Pos2); 3] {
        let radius = (self.inner_radius + self.outer_radius) * 0.57;
        [
            (
                TacticalAction::OpenInExplorer,
                self.center + Vec2::angled(-2.35) * radius,
            ),
            (
                TacticalAction::Recycle,
                self.center + Vec2::angled(-1.05) * radius,
            ),
            (
                TacticalAction::DeletePermanently,
                self.center + Vec2::angled(0.25) * radius,
            ),
        ]
    }

    pub fn hit_test(self, pointer: Pos2) -> Option<TacticalAction> {
        let distance = pointer.distance(self.center);
        if distance < self.inner_radius || distance > self.outer_radius {
            return None;
        }
        self.action_centers()
            .into_iter()
            .min_by(|(_, a), (_, b)| a.distance(pointer).total_cmp(&b.distance(pointer)))
            .filter(|(_, center)| center.distance(pointer) <= 31.0)
            .map(|(action, _)| action)
    }
}

#[derive(Clone, Debug)]
pub struct TacticalArcState {
    pub target: ContextTarget,
    pub geometry: TacticalArcGeometry,
    keyboard_index: Option<usize>,
    armed: bool,
}

impl TacticalArcState {
    pub fn new(target: ContextTarget, pointer: Pos2, work_area: Rect) -> Self {
        Self {
            target,
            geometry: TacticalArcGeometry::clamped(pointer, work_area, 132.0),
            keyboard_index: None,
            armed: false,
        }
    }

    pub fn show(&mut self, context: &egui::Context, font: FontId) -> Option<TacticalArcOutcome> {
        let mut chosen = None;
        let ignore_opening_secondary_click = !self.armed;
        self.armed = true;
        let area_rect = self.geometry.bounds().expand(8.0);
        let response = egui::Area::new(Id::new("tactical-arc"))
            .order(egui::Order::Foreground)
            .fixed_pos(area_rect.min)
            .show(context, |ui| {
                ui.set_min_size(area_rect.size());
                let local_center = self.geometry.center - area_rect.min.to_vec2();
                let local_geometry = TacticalArcGeometry::new(local_center, 36.0, 132.0);
                let (rect, response) = ui.allocate_exact_size(area_rect.size(), Sense::click());
                let painter = ui.painter_at(rect);
                painter.circle_filled(
                    local_center,
                    132.0,
                    Color32::from_rgba_unmultiplied(5, 8, 10, 240),
                );
                painter.circle_stroke(local_center, 132.0, Stroke::new(1.0, hud::HAIRLINE));
                painter.circle_stroke(local_center, 36.0, Stroke::new(1.0, hud::ORANGE));
                painter.text(
                    local_center,
                    Align2::CENTER_CENTER,
                    "ARC",
                    font.clone(),
                    hud::ORANGE,
                );
                let pointer = ui
                    .ctx()
                    .pointer_hover_pos()
                    .map(|p| p - area_rect.min.to_vec2());
                let hovered = pointer.and_then(|p| local_geometry.hit_test(p));
                if let Some(pointer) = pointer {
                    painter.line_segment([local_center, pointer], Stroke::new(1.0, hud::CYAN));
                }
                for (index, (action, center)) in
                    local_geometry.action_centers().into_iter().enumerate()
                {
                    let active = hovered == Some(action)
                        || (hovered.is_none() && self.keyboard_index == Some(index));
                    let color = if action == TacticalAction::DeletePermanently {
                        hud::ORANGE
                    } else if active {
                        hud::LIME
                    } else {
                        hud::CYAN
                    };
                    painter.circle_filled(center, 29.0, hud::PANEL_RAISED);
                    painter.circle_stroke(
                        center,
                        29.0,
                        Stroke::new(if active { 2.0 } else { 1.0 }, color),
                    );
                    painter.text(
                        center + Vec2::new(0.0, -3.0),
                        Align2::CENTER_CENTER,
                        action.shortcut(),
                        font.clone(),
                        color,
                    );
                    painter.text(
                        center + Vec2::new(0.0, 40.0),
                        Align2::CENTER_TOP,
                        action.label(),
                        font.clone(),
                        color,
                    );
                    let action_response = ui.interact(
                        Rect::from_center_size(center, Vec2::splat(58.0)),
                        Id::new("tactical-action").with(index),
                        Sense::click(),
                    );
                    action_response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("{} · key {}", action.label(), action.shortcut()),
                        )
                    });
                }
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
            let forward = input.key_pressed(Key::ArrowDown)
                || input.key_pressed(Key::ArrowRight)
                || (input.key_pressed(Key::Tab) && !input.modifiers.shift);
            let backward = input.key_pressed(Key::ArrowUp)
                || input.key_pressed(Key::ArrowLeft)
                || (input.key_pressed(Key::Tab) && input.modifiers.shift);
            if forward {
                self.keyboard_index = Some(
                    self.keyboard_index
                        .map_or(0, |index| (index + 1) % TacticalAction::ALL.len()),
                );
            }
            if backward {
                self.keyboard_index = Some(
                    self.keyboard_index
                        .map_or(TacticalAction::ALL.len() - 1, |index| {
                            (index + TacticalAction::ALL.len() - 1) % TacticalAction::ALL.len()
                        }),
                );
            }
            if (input.key_pressed(Key::Enter) || input.key_pressed(Key::Space))
                && let Some(index) = self.keyboard_index
            {
                chosen = Some(TacticalArcOutcome::Action(TacticalAction::ALL[index]));
            }
            if input.key_pressed(Key::Num1) {
                chosen = Some(TacticalArcOutcome::Action(TacticalAction::OpenInExplorer));
            }
            if input.key_pressed(Key::Num2) {
                chosen = Some(TacticalArcOutcome::Action(TacticalAction::Recycle));
            }
            if input.key_pressed(Key::Num3) {
                chosen = Some(TacticalArcOutcome::Action(
                    TacticalAction::DeletePermanently,
                ));
            }
        });
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_and_outer_dead_zones_do_not_activate_actions() {
        let arc = TacticalArcGeometry::new(Pos2::new(200.0, 200.0), 42.0, 112.0);
        assert_eq!(arc.hit_test(Pos2::new(200.0, 200.0)), None);
        assert_eq!(arc.hit_test(Pos2::new(400.0, 400.0)), None);
    }

    #[test]
    fn clamped_arc_stays_inside_work_area() {
        let area = Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0));
        let arc = TacticalArcGeometry::clamped(Pos2::new(4.0, 4.0), area, 112.0);
        assert!(area.contains(arc.bounds().min));
        assert!(area.contains(arc.bounds().max));
    }
}
