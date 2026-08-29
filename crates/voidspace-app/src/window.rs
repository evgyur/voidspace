pub fn main_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Voidspace")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([800.0, 600.0])
        .with_maximized(true)
}

#[derive(Debug)]
pub(crate) struct StartupMaximizer {
    pending: bool,
}

impl StartupMaximizer {
    pub(crate) fn new() -> Self {
        Self { pending: true }
    }

    pub(crate) fn apply(&mut self, context: &egui::Context) {
        if std::mem::take(&mut self.pending) {
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{ViewportCommand, ViewportId};

    #[test]
    fn main_window_opens_maximized() {
        assert_eq!(super::main_viewport().maximized, Some(true));
    }

    #[test]
    fn post_creation_maximize_is_sent_once_after_the_hidden_startup_window_exists() {
        let context = egui::Context::default();
        let mut maximizer = super::StartupMaximizer::new();
        let mut first = context.run_ui(Default::default(), |ui| maximizer.apply(ui.ctx()));
        assert!(
            first
                .viewport_output
                .get(&ViewportId::ROOT)
                .expect("root viewport output")
                .commands
                .iter()
                .any(|command| matches!(command, ViewportCommand::Maximized(true)))
        );
        first.textures_delta.clear();

        let mut second = context.run_ui(Default::default(), |ui| maximizer.apply(ui.ctx()));
        assert!(
            second
                .viewport_output
                .get(&ViewportId::ROOT)
                .expect("root viewport output")
                .commands
                .iter()
                .all(|command| !matches!(command, ViewportCommand::Maximized(_)))
        );
        second.textures_delta.clear();
    }
}
