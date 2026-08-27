use egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2,
};

pub const GRID: Color32 = Color32::from_rgb(29, 34, 37);
pub const CYAN: Color32 = Color32::from_rgb(30, 205, 226);
pub const LIME: Color32 = Color32::from_rgb(189, 255, 62);
pub const ORANGE: Color32 = Color32::from_rgb(255, 83, 43);
pub const MAGENTA: Color32 = Color32::from_rgb(255, 73, 188);
pub const PANEL: Color32 = Color32::from_rgb(11, 14, 16);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(16, 20, 22);
pub const HAIRLINE: Color32 = Color32::from_rgb(48, 55, 59);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudState {
    Neutral,
    Active,
    Warning,
    Danger,
}

impl HudState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Neutral => "IDLE",
            Self::Active => "ACTIVE",
            Self::Warning => "WARNING",
            Self::Danger => "DANGER",
        }
    }

    pub const fn color(self) -> Color32 {
        match self {
            Self::Neutral => Color32::from_rgb(142, 148, 157),
            Self::Active => LIME,
            Self::Warning => ORANGE,
            Self::Danger => MAGENTA,
        }
    }
}

pub fn cut_corner_points(rect: Rect, cut: f32) -> [Pos2; 6] {
    let cut = cut.clamp(0.0, rect.width().min(rect.height()) * 0.25);
    [
        rect.left_top(),
        Pos2::new(rect.right() - cut, rect.top()),
        Pos2::new(rect.right(), rect.top() + cut),
        rect.right_bottom(),
        Pos2::new(rect.left() + cut, rect.bottom()),
        rect.left_bottom(),
    ]
}

pub fn paint_cut_frame(painter: &Painter, rect: Rect, fill: Color32, stroke: Stroke, cut: f32) {
    let points = cut_corner_points(rect, cut).to_vec();
    painter.add(Shape::convex_polygon(points, fill, stroke));
}

pub fn paint_state_square(painter: &Painter, origin: Pos2, state: HudState) {
    let rect = Rect::from_min_size(origin, Vec2::splat(6.0));
    painter.rect_filled(rect, 0.0, state.color());
}

pub fn paint_corner_brackets(painter: &Painter, rect: Rect, color: Color32) {
    let length = 13.0_f32.min(rect.width() * 0.22).min(rect.height() * 0.22);
    let stroke = Stroke::new(2.0, color);
    for (corner, x_sign, y_sign) in [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.left_bottom(), 1.0, -1.0),
        (rect.right_bottom(), -1.0, -1.0),
    ] {
        painter.line_segment([corner, corner + Vec2::new(length * x_sign, 0.0)], stroke);
        painter.line_segment([corner, corner + Vec2::new(0.0, length * y_sign)], stroke);
    }
}

pub fn paint_reticle(painter: &Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    painter.circle_stroke(center, 5.0, stroke);
    painter.line_segment(
        [
            center + Vec2::new(-10.0, 0.0),
            center + Vec2::new(-4.0, 0.0),
        ],
        stroke,
    );
    painter.line_segment(
        [center + Vec2::new(4.0, 0.0), center + Vec2::new(10.0, 0.0)],
        stroke,
    );
    painter.line_segment(
        [
            center + Vec2::new(0.0, -10.0),
            center + Vec2::new(0.0, -4.0),
        ],
        stroke,
    );
    painter.line_segment(
        [center + Vec2::new(0.0, 4.0), center + Vec2::new(0.0, 10.0)],
        stroke,
    );
}

pub struct InstrumentCell<'a> {
    pub eyebrow: &'a str,
    pub value: &'a str,
    pub state: HudState,
    pub width: f32,
}

pub fn instrument_cell(
    ui: &mut Ui,
    cell: InstrumentCell<'_>,
    label_font: FontId,
    value_font: FontId,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(cell.width, 34.0), Sense::click());
    let painter = ui.painter_at(rect);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("{} · {} · {}", cell.eyebrow, cell.value, cell.state.label()),
        )
    });
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
    paint_state_square(&painter, rect.left_top() + Vec2::new(8.0, 8.0), cell.state);
    painter.text(
        rect.left_top() + Vec2::new(20.0, 6.0),
        Align2::LEFT_TOP,
        cell.eyebrow,
        label_font,
        Color32::from_rgb(142, 148, 157),
    );
    painter.text(
        rect.left_bottom() + Vec2::new(8.0, -6.0),
        Align2::LEFT_BOTTOM,
        cell.value,
        value_font,
        if cell.state == HudState::Neutral {
            Color32::WHITE
        } else {
            cell.state.color()
        },
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_corner_points_stay_inside_source_rect() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 60.0));
        for point in cut_corner_points(rect, 8.0) {
            assert!(rect.contains(point));
        }
    }

    #[test]
    fn semantic_states_have_text_cues() {
        assert_eq!(HudState::Active.label(), "ACTIVE");
        assert_eq!(HudState::Warning.label(), "WARNING");
        assert_eq!(HudState::Danger.label(), "DANGER");
    }
}
