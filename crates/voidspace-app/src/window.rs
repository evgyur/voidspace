pub fn main_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Voidspace")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([800.0, 600.0])
        .with_maximized(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_opens_maximized() {
        assert_eq!(super::main_viewport().maximized, Some(true));
    }
}
