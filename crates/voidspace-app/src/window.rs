const STARTUP_WINDOW_SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

pub fn main_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Voidspace")
        .with_inner_size(STARTUP_WINDOW_SIZE)
        .with_min_inner_size([1100.0, 700.0])
        .with_maximized(false)
}

#[derive(Debug)]
pub(crate) struct StartupWindowSizer {
    phase: StartupWindowPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupWindowPhase {
    Resize,
    Center,
    Complete,
}

impl StartupWindowSizer {
    pub(crate) fn new() -> Self {
        Self {
            phase: StartupWindowPhase::Resize,
        }
    }

    pub(crate) fn apply(&mut self, context: &egui::Context) {
        match self.phase {
            StartupWindowPhase::Resize => {
                context.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                context.send_viewport_cmd(egui::ViewportCommand::InnerSize(STARTUP_WINDOW_SIZE));
                self.phase = StartupWindowPhase::Center;
                context.request_repaint();
            }
            StartupWindowPhase::Center => {
                if let Some(command) = egui::ViewportCommand::center_on_screen(context) {
                    context.send_viewport_cmd(command);
                    self.phase = StartupWindowPhase::Complete;
                } else {
                    context.request_repaint();
                }
            }
            StartupWindowPhase::Complete => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{ViewportCommand, ViewportId};

    #[test]
    fn main_window_opens_as_a_large_landscape_window_not_fullscreen() {
        let viewport = super::main_viewport();
        assert_eq!(viewport.maximized, Some(false));
        assert_eq!(viewport.inner_size, Some(egui::vec2(1440.0, 900.0)));
        assert_eq!(viewport.min_inner_size, Some(egui::vec2(1100.0, 700.0)));
    }

    #[test]
    fn post_creation_landscape_size_is_followed_by_one_centering_command() {
        let context = egui::Context::default();
        let mut sizer = super::StartupWindowSizer::new();
        let mut first = context.run_ui(Default::default(), |ui| sizer.apply(ui.ctx()));
        let commands = &first
            .viewport_output
            .get(&ViewportId::ROOT)
            .expect("root viewport output")
            .commands;
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Maximized(false)))
        );
        assert!(
            commands.iter().any(
                |command| matches!(command, ViewportCommand::InnerSize(size) if *size == super::STARTUP_WINDOW_SIZE)
            )
        );
        first.textures_delta.clear();

        let mut input = egui::RawInput::default();
        input.viewports.insert(
            ViewportId::ROOT,
            egui::ViewportInfo {
                monitor_size: Some(egui::vec2(2560.0, 1440.0)),
                outer_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(900.0, 400.0),
                    egui::vec2(1456.0, 939.0),
                )),
                ..Default::default()
            },
        );
        let mut second = context.run_ui(input, |ui| sizer.apply(ui.ctx()));
        let second_commands = &second
            .viewport_output
            .get(&ViewportId::ROOT)
            .expect("root viewport output")
            .commands;
        assert!(
            second_commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::OuterPosition(position) if *position == egui::pos2(552.0, 250.5)))
        );
        assert!(second_commands.iter().all(|command| !matches!(
            command,
            ViewportCommand::Maximized(_) | ViewportCommand::InnerSize(_)
        )));
        second.textures_delta.clear();

        let mut third = context.run_ui(Default::default(), |ui| sizer.apply(ui.ctx()));
        assert!(
            third
                .viewport_output
                .get(&ViewportId::ROOT)
                .expect("root viewport output")
                .commands
                .iter()
                .all(|command| !matches!(
                    command,
                    ViewportCommand::Maximized(_)
                        | ViewportCommand::InnerSize(_)
                        | ViewportCommand::OuterPosition(_)
                ))
        );
        third.textures_delta.clear();
    }
}
