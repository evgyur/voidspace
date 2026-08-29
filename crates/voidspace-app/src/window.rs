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
    pending: bool,
}

impl StartupWindowSizer {
    pub(crate) fn new() -> Self {
        Self { pending: true }
    }

    pub(crate) fn apply(&mut self, context: &egui::Context) {
        if std::mem::take(&mut self.pending) {
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            context.send_viewport_cmd(egui::ViewportCommand::InnerSize(STARTUP_WINDOW_SIZE));
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
    fn post_creation_landscape_size_is_sent_once_after_the_hidden_startup_window_exists() {
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

        let mut second = context.run_ui(Default::default(), |ui| sizer.apply(ui.ctx()));
        assert!(
            second
                .viewport_output
                .get(&ViewportId::ROOT)
                .expect("root viewport output")
                .commands
                .iter()
                .all(|command| !matches!(
                    command,
                    ViewportCommand::Maximized(_) | ViewportCommand::InnerSize(_)
                ))
        );
        second.textures_delta.clear();
    }
}
