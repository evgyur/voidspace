use crate::{hud, theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum StatusKind {
    Scan,
    Engine,
    FileOp,
    Notice,
    DiskUsed,
    Indexed,
    Entries,
    Watch,
    Filter,
}

impl StatusKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Scan => "SCAN",
            Self::Engine => "ENGINE",
            Self::FileOp => "FILE OP",
            Self::Notice => "NOTICE",
            Self::DiskUsed => "DISK USED",
            Self::Indexed => "INDEXED",
            Self::Entries => "ENTRIES",
            Self::Watch => "WATCH",
            Self::Filter => "FILTER",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::Scan => 9,
            Self::Engine => 8,
            Self::FileOp => 7,
            Self::Notice => 6,
            Self::DiskUsed => 5,
            Self::Indexed => 4,
            Self::Entries => 3,
            Self::Watch => 2,
            Self::Filter => 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusModule {
    pub kind: StatusKind,
    pub value: String,
    pub state: hud::HudState,
}

pub struct StatusLayout<'a> {
    pub visible: Vec<&'a StatusModule>,
    pub hidden: Vec<&'a StatusModule>,
}

fn estimated_width(module: &StatusModule) -> f32 {
    let chars = module
        .kind
        .label()
        .chars()
        .count()
        .max(module.value.chars().count()) as f32;
    (chars * 7.1 + 34.0).clamp(92.0, 230.0)
}

pub fn layout_modules(width: f32, modules: &[StatusModule]) -> StatusLayout<'_> {
    let mut ordered: Vec<_> = modules.iter().collect();
    ordered.sort_by_key(|module| std::cmp::Reverse(module.kind.priority()));
    let mut visible = Vec::new();
    let mut hidden = Vec::new();
    let mut used = 0.0;
    for module in ordered {
        let reserve_more = if visible.len() + hidden.len() + 1 < modules.len() {
            100.0
        } else {
            0.0
        };
        let module_width = estimated_width(module);
        if used + module_width + reserve_more <= width
            || matches!(module.kind, StatusKind::Scan | StatusKind::Engine)
        {
            used += module_width;
            visible.push(module);
        } else {
            hidden.push(module);
        }
    }
    StatusLayout { visible, hidden }
}

pub fn show(
    ui: &mut egui::Ui,
    typography: &theme::Typography,
    modules: &[StatusModule],
) -> Option<egui::Id> {
    let layout = layout_modules(ui.available_width(), modules);
    let mut more_focus = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for module in layout.visible {
            let width = estimated_width(module);
            hud::instrument_cell(
                ui,
                hud::InstrumentCell {
                    eyebrow: module.kind.label(),
                    value: &module.value,
                    state: module.state,
                    width,
                },
                typography.font(theme::TypographyToken::StatusLabel),
                typography.font(theme::TypographyToken::StatusValue),
            );
        }
        if !layout.hidden.is_empty() {
            let id = ui.make_persistent_id("status-more");
            let rect = ui.allocate_space(egui::vec2(100.0, 34.0)).1;
            let response = ui.interact(rect, id, egui::Sense::click());
            let rect = response.rect;
            hud::paint_cut_frame(
                &ui.painter_at(rect),
                rect,
                hud::PANEL_RAISED,
                egui::Stroke::new(1.0, hud::ORANGE),
                7.0,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("MORE +{}", layout.hidden.len()),
                typography.font(theme::TypographyToken::StatusValue),
                hud::ORANGE,
            );
            if response.clicked()
                || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                more_focus = Some(id);
            }
        }
    });
    more_focus
}

pub fn show_details(
    context: &egui::Context,
    typography: &theme::Typography,
    modules: &[StatusModule],
) -> bool {
    let mut open = true;
    egui::Window::new("STATUS DETAILS")
        .id(egui::Id::new("status-details"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(360.0)
        .frame(
            egui::Frame::new()
                .fill(hud::PANEL)
                .stroke(egui::Stroke::new(1.0, hud::ORANGE))
                .inner_margin(egui::Margin::same(14)),
        )
        .show(context, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for module in modules {
                    hud::instrument_cell(
                        ui,
                        hud::InstrumentCell {
                            eyebrow: module.kind.label(),
                            value: &module.value,
                            state: module.state,
                            width: ui.available_width(),
                        },
                        typography.font(theme::TypographyToken::StatusLabel),
                        typography.font(theme::TypographyToken::StatusValue),
                    );
                }
            });
        });
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(kind: StatusKind) -> StatusModule {
        StatusModule {
            kind,
            value: "VALUE".to_owned(),
            state: hud::HudState::Neutral,
        }
    }

    #[test]
    fn status_keeps_scan_and_engine_then_collapses_low_priority_items() {
        let modules = [
            module(StatusKind::Scan),
            module(StatusKind::Engine),
            module(StatusKind::FileOp),
            module(StatusKind::Notice),
            module(StatusKind::DiskUsed),
            module(StatusKind::Indexed),
            module(StatusKind::Entries),
            module(StatusKind::Watch),
            module(StatusKind::Filter),
        ];
        let layout = layout_modules(420.0, &modules);
        assert!(
            layout
                .visible
                .iter()
                .any(|module| module.kind == StatusKind::Scan)
        );
        assert!(
            layout
                .visible
                .iter()
                .any(|module| module.kind == StatusKind::Engine)
        );
        assert!(!layout.hidden.is_empty());
    }
}
