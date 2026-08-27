use std::path::{Path, PathBuf};

use egui::{Align2, Sense, Stroke, StrokeKind, WidgetInfo, WidgetType};

use crate::{theme, volume};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VolumeRootKey(u8);

impl VolumeRootKey {
    pub fn new(letter: char) -> Option<Self> {
        letter
            .is_ascii_alphabetic()
            .then_some(Self(letter.to_ascii_uppercase() as u8))
    }

    pub fn from_scan_root(path: &Path) -> Option<Self> {
        let text = path.as_os_str().to_str()?;
        let bytes = text.as_bytes();
        (bytes.len() == 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\')
            .then(|| Self(bytes[0].to_ascii_uppercase()))
    }

    pub fn path(self) -> PathBuf {
        PathBuf::from(format!("{}:\\", self.0 as char))
    }

    pub fn display(self) -> String {
        format!("{}:\\", self.0 as char)
    }
}

pub struct ScopePresentation<'a> {
    full: &'a str,
    focused: bool,
}

impl<'a> ScopePresentation<'a> {
    pub fn new(full: &'a str, focused: bool) -> Self {
        Self { full, focused }
    }

    pub fn visible_text(&self) -> &str {
        if self.focused {
            return self.full;
        }
        let bytes = self.full.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            &self.full[2..]
        } else {
            self.full
        }
    }
}

pub fn repair_volume_focus(
    previous: Option<VolumeRootKey>,
    previous_index: usize,
    roots: &[VolumeRootKey],
) -> Option<VolumeRootKey> {
    if let Some(root) = previous.filter(|root| roots.contains(root)) {
        return Some(root);
    }
    roots.get(previous_index).or_else(|| roots.last()).copied()
}

#[derive(Clone, Debug, Default)]
pub struct VolumeSwitcherState {
    pub open: bool,
    pub scope_editing: bool,
    pub focused: Option<VolumeRootKey>,
    pub focused_index: usize,
    pub issue: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VolumeSwitcherAction {
    None,
    Close,
    OpenOrActivate(PathBuf),
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut egui::Ui,
    scope_text: &mut String,
    state: &mut VolumeSwitcherState,
    volumes: &[volume::VolumeInfo],
    volume_ids: &[String],
    active: Option<VolumeRootKey>,
    active_volume_id: Option<&str>,
    refresh_in_flight: bool,
    refresh_error: Option<&str>,
    typography: &theme::Typography,
) -> VolumeSwitcherAction {
    let roots = volumes
        .iter()
        .filter_map(|volume| VolumeRootKey::from_scan_root(&volume.root_path))
        .collect::<Vec<_>>();
    let mut action = VolumeSwitcherAction::None;
    let mut anchor = egui::Rect::NOTHING;
    let arrow = if state.open { '▴' } else { '▾' };
    let drive_label = active
        .map(|root| {
            format!(
                "{} · {} {arrow}",
                active_volume_id.unwrap_or("VOL:--"),
                root.display()
            )
        })
        .unwrap_or_else(|| format!("DISKS {arrow}"));

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let drive = ui.add_sized(
            [106.0, 36.0],
            egui::Button::new(
                egui::RichText::new(drive_label)
                    .font(typography.font(theme::TypographyToken::UiControl))
                    .color(if state.open { theme::BG } else { theme::TEXT }),
            )
            .fill(if state.open {
                theme::ORANGE
            } else {
                theme::RAISED
            })
            .stroke(Stroke::new(1.0, theme::ORANGE)),
        );
        anchor = drive.rect;
        if drive.clicked() {
            state.open = !state.open;
            state.scope_editing = false;
            state.issue = None;
            if state.open {
                state.focused = repair_volume_focus(active, 0, &roots);
                state.focused_index = state
                    .focused
                    .and_then(|focused| roots.iter().position(|root| *root == focused))
                    .unwrap_or(0);
            }
        }

        let scope_width = ui.available_width().max(72.0);
        if state.scope_editing {
            let editor_id = ui.make_persistent_id("voidspace_scope_editor");
            let response = ui.add_sized(
                [scope_width, 36.0],
                egui::TextEdit::singleline(scope_text)
                    .id(editor_id)
                    .font(typography.font(theme::TypographyToken::DataNormal))
                    .margin(egui::Margin::symmetric(11, 8)),
            );
            if !response.has_focus() {
                response.request_focus();
            }
            if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                action = VolumeSwitcherAction::OpenOrActivate(PathBuf::from(scope_text.trim()));
            } else if response.lost_focus() {
                state.scope_editing = false;
            }
        } else {
            let presentation = ScopePresentation::new(scope_text, false);
            let visible = presentation.visible_text();
            let response = ui.add_sized(
                [scope_width, 36.0],
                egui::Button::new(
                    egui::RichText::new(if visible.is_empty() { "\\" } else { visible })
                        .font(typography.font(theme::TypographyToken::DataNormal))
                        .color(theme::TEXT),
                )
                .fill(theme::TILE_BG)
                .stroke(Stroke::new(1.0, theme::LINE)),
            );
            if response.clicked() {
                state.scope_editing = true;
            }
            response.on_hover_text("Click to edit the full disk or folder path");
        }
    });

    if !state.open {
        return action;
    }

    if !roots.is_empty() {
        if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
            state.focused_index = (state.focused_index + 1).min(roots.len() - 1);
            state.focused = Some(roots[state.focused_index]);
        }
        if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
            state.focused_index = state.focused_index.saturating_sub(1);
            state.focused = Some(roots[state.focused_index]);
        }
        if ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        }) && let Some(focused) = state.focused
        {
            action = VolumeSwitcherAction::OpenOrActivate(focused.path());
        }
    }
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        return VolumeSwitcherAction::Close;
    }

    let viewport = ui.ctx().content_rect().shrink(8.0);
    let width = 390.0_f32.min(viewport.width());
    let below = viewport.bottom() - anchor.bottom();
    let above = anchor.top() - viewport.top();
    let opens_below = below >= 220.0 || below >= above;
    let available_height = (if opens_below { below } else { above } - 6.0)
        .clamp(120.0, 460.0)
        .min(viewport.height());
    let x = anchor
        .left()
        .clamp(viewport.left(), viewport.right() - width);
    let y = if opens_below {
        (anchor.bottom() + 4.0).min(viewport.bottom() - available_height)
    } else {
        (anchor.top() - available_height - 4.0).max(viewport.top())
    };
    let popup = egui::Area::new(egui::Id::new("voidspace_volume_switcher_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(x, y))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(theme::SURFACE)
                .stroke(Stroke::new(1.0, theme::ORANGE))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(width - 24.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("VOLUMES")
                                .font(typography.font(theme::TypographyToken::DisplayView))
                                .color(theme::TEXT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(if refresh_in_flight {
                                    "REFRESHING"
                                } else {
                                    "LIVE"
                                })
                                .font(typography.font(theme::TypographyToken::DataMicro))
                                .color(if refresh_in_flight {
                                    theme::ORANGE
                                } else {
                                    theme::LIME
                                }),
                            );
                        });
                    });
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height((available_height - 76.0).max(56.0))
                        .show(ui, |ui| {
                            if volumes.is_empty() {
                                ui.label(
                                    egui::RichText::new("Detecting mounted Windows volumes…")
                                        .font(typography.font(theme::TypographyToken::UiBody))
                                        .color(theme::MUTED),
                                );
                            }
                            for (volume, volume_id) in volumes.iter().zip(volume_ids) {
                                let root = VolumeRootKey::from_scan_root(&volume.root_path);
                                let is_current = root == active;
                                let focused = root == state.focused;
                                if volume_row(
                                    ui, volume, volume_id, is_current, focused, typography,
                                ) {
                                    action = VolumeSwitcherAction::OpenOrActivate(
                                        volume.root_path.clone(),
                                    );
                                }
                                ui.add_space(6.0);
                            }
                        });
                    if let Some(issue) = state.issue.as_deref().or(refresh_error) {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(issue)
                                .font(typography.font(theme::TypographyToken::DataMicro))
                                .color(theme::ORANGE),
                        );
                    }
                });
        });

    if ui.input(|input| input.pointer.any_pressed())
        && let Some(position) = ui.input(|input| input.pointer.interact_pos())
        && !popup.response.rect.contains(position)
        && !anchor.contains(position)
    {
        VolumeSwitcherAction::Close
    } else {
        action
    }
}

fn volume_row(
    ui: &mut egui::Ui,
    volume: &volume::VolumeInfo,
    volume_id: &str,
    current: bool,
    focused: bool,
    typography: &theme::Typography,
) -> bool {
    let width = ui.available_width();
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 68.0), Sense::click());
    let rect = response.rect;
    let highlighted = response.hovered() || focused;
    painter.rect(
        rect,
        0.0,
        if highlighted {
            theme::RAISED
        } else {
            theme::TILE_BG
        },
        Stroke::new(
            if current || focused { 1.5 } else { 1.0 },
            if current {
                theme::ORANGE
            } else if focused {
                theme::CYAN
            } else {
                theme::LINE
            },
        ),
        StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(rect.left() + 12.0, rect.top() + 10.0),
        Align2::LEFT_TOP,
        format!("{volume_id} · {}  {}", volume.display_root, volume.label),
        typography.font(theme::TypographyToken::UiControl),
        theme::TEXT,
    );
    painter.text(
        egui::pos2(rect.right() - 12.0, rect.top() + 11.0),
        Align2::RIGHT_TOP,
        format!("FREE {}", volume::format_decimal_bytes(volume.usage.free)),
        typography.font(theme::TypographyToken::DataCompact),
        theme::TILE_MUTED,
    );
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 12.0, rect.bottom() - 14.0),
        egui::pos2(rect.right() - 12.0, rect.bottom() - 9.0),
    );
    painter.rect_filled(bar, 0.0, theme::LINE);
    let used = volume::used_ratio(volume.usage);
    painter.rect_filled(
        egui::Rect::from_min_max(
            bar.min,
            egui::pos2(bar.left() + bar.width() * used, bar.bottom()),
        ),
        0.0,
        if current { theme::ORANGE } else { theme::CYAN },
    );
    let verb = if current {
        "current volume"
    } else {
        "switch existing tab or start scan"
    };
    response.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Button,
            true,
            format!(
                "{volume_id}, {} {}, total {}, free {}, {verb}",
                volume.display_root,
                volume.label,
                volume::format_decimal_bytes(volume.usage.total),
                volume::format_decimal_bytes(volume.usage.free)
            ),
        )
    });
    response.clicked()
}

pub(crate) fn matching_volume_tab_index<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    requested: &Path,
) -> Option<usize> {
    let requested = VolumeRootKey::from_scan_root(requested)?;
    paths
        .into_iter()
        .position(|path| VolumeRootKey::from_scan_root(path) == Some(requested))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_tab_matching_is_case_folded_lowest_index_and_root_only() {
        let tabs = [Path::new(r"C:\data"), Path::new(r"h:\"), Path::new(r"H:\")];
        assert_eq!(matching_volume_tab_index(tabs, Path::new(r"H:\")), Some(1));
        assert_eq!(
            matching_volume_tab_index([Path::new(r"H:\books")], Path::new(r"H:\")),
            None
        );
        assert_eq!(matching_volume_tab_index(tabs, Path::new("H:")), None);
    }
}
