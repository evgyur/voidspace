use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(7, 8, 9);
pub const MAP_BG: Color32 = Color32::from_rgb(9, 10, 12);
pub const SURFACE: Color32 = Color32::from_rgb(13, 15, 17);
pub const RAISED: Color32 = Color32::from_rgb(17, 20, 25);
pub const TILE_BG: Color32 = Color32::from_rgb(11, 12, 14);
pub const FILTERED_TILE: Color32 = Color32::from_rgb(20, 22, 25);
pub const LINE: Color32 = Color32::from_rgb(42, 46, 52);
pub const TEXT: Color32 = Color32::from_rgb(242, 243, 245);
pub const MUTED: Color32 = Color32::from_rgb(142, 148, 157);
pub const TILE_MUTED: Color32 = Color32::from_rgb(183, 187, 193);
pub const ORANGE: Color32 = Color32::from_rgb(255, 90, 47);
pub const CYAN: Color32 = Color32::from_rgb(25, 211, 255);
pub const LIME: Color32 = Color32::from_rgb(201, 246, 90);
pub const MAGENTA: Color32 = Color32::from_rgb(255, 78, 205);
pub const VIOLET: Color32 = Color32::from_rgb(139, 92, 246);

pub fn install(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_font = [
        r"C:\Windows\Fonts\SegUIVar.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
    ]
    .into_iter()
    .find_map(|path| std::fs::read(path).ok());
    if let Some(bytes) = windows_font {
        fonts
            .font_data
            .insert("segoe".into(), FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
    }
    context.set_fonts(fonts);
    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    style.visuals = Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = BG;
    style.visuals.faint_bg_color = RAISED;
    style.visuals.widgets.inactive.bg_fill = RAISED;
    style.visuals.widgets.inactive.bg_stroke.color = LINE;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(30, 33, 38);
    style.visuals.widgets.hovered.bg_stroke.color = ORANGE;
    style.visuals.widgets.active.bg_fill = ORANGE;
    style.visuals.widgets.active.fg_stroke.color = BG;
    style.visuals.selection.bg_fill = ORANGE;
    style.visuals.selection.stroke.color = BG;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(22.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    context.set_style_of(egui::Theme::Dark, style);
    context.set_theme(egui::Theme::Dark);
}
