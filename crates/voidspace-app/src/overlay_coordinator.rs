#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransientOverlay {
    TacticalArc,
    DiskPicker,
    About,
    CompactFilter,
    InspectorDrawer,
    StatusDetails,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalOverlay {
    PermanentDelete,
}

#[derive(Default)]
pub struct OverlayCoordinator {
    transient: Option<TransientOverlay>,
    modal: Option<ModalOverlay>,
    restore_focus: Option<egui::Id>,
}

impl OverlayCoordinator {
    pub fn transient(&self) -> Option<TransientOverlay> {
        self.transient
    }

    pub fn modal(&self) -> Option<ModalOverlay> {
        self.modal
    }

    pub fn open_transient(&mut self, overlay: TransientOverlay, restore_focus: Option<egui::Id>) {
        if self.modal.is_none() {
            self.transient = Some(overlay);
            self.restore_focus = restore_focus;
        }
    }

    pub fn open_modal(&mut self, overlay: ModalOverlay) {
        self.transient = None;
        self.restore_focus = None;
        self.modal = Some(overlay);
    }

    pub fn close_transient(&mut self, context: &egui::Context) {
        self.transient = None;
        if let Some(id) = self.restore_focus.take() {
            context.memory_mut(|memory| memory.request_focus(id));
        }
    }

    pub fn dismiss_transient(&mut self) {
        self.transient = None;
        self.restore_focus = None;
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    pub fn owns_pointer(&self) -> bool {
        self.modal.is_some() || self.transient.is_some()
    }

    pub fn route_escape(&mut self, context: &egui::Context) -> bool {
        if !context.input(|input| input.key_pressed(egui::Key::Escape)) {
            return false;
        }
        if self.modal.is_some() {
            return false;
        }
        if self.transient.is_some() {
            self.close_transient(context);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_replaces_transient_and_transient_is_exclusive() {
        let mut overlays = OverlayCoordinator::default();
        overlays.open_transient(TransientOverlay::DiskPicker, None);
        overlays.open_transient(TransientOverlay::StatusDetails, None);
        assert_eq!(overlays.transient(), Some(TransientOverlay::StatusDetails));
        overlays.open_modal(ModalOverlay::PermanentDelete);
        assert_eq!(overlays.modal(), Some(ModalOverlay::PermanentDelete));
        assert_eq!(overlays.transient(), None);
    }
}
