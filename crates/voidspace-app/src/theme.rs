use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, FontTweak, TextStyle, Visuals,
    epaint::text::VariationCoords,
};
use sha2::{Digest, Sha256};
use skrifa::{FontRef, MetadataProvider};

const UNBOUNDED: &[u8] = include_bytes!("../assets/fonts/Unbounded[wght].ttf");
const GOLOS: &[u8] = include_bytes!("../assets/fonts/GolosText[wght].ttf");
const JETBRAINS: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono[wght].ttf");
const CYRILLIC_SENTINELS: &str = "КАРТА ДИСКАСвободноУдалить навсегда";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypographySource {
    EmbeddedSelected,
    EmbeddedFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypographyToken {
    DisplayBrand,
    DisplayView,
    DisplayAction,
    UiTitle,
    UiBody,
    UiControl,
    TileNameLarge,
    TileNameCompact,
    DataNormal,
    DataCompact,
    DataMicro,
    StatusLabel,
    StatusValue,
}

#[derive(Clone, Debug)]
pub struct Typography {
    source: TypographySource,
    epoch: u64,
    pixels_per_point: f32,
}

impl Typography {
    #[cfg(test)]
    fn for_test(source: TypographySource, pixels_per_point: f32) -> Self {
        Self {
            source,
            epoch: 1,
            pixels_per_point,
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn source(&self) -> TypographySource {
        self.source
    }

    pub fn update_pixels_per_point(&mut self, value: f32) -> bool {
        if (self.pixels_per_point - value).abs() <= f32::EPSILON {
            return false;
        }
        self.pixels_per_point = value;
        self.epoch = self.epoch.saturating_add(1);
        true
    }

    pub fn font(&self, token: TypographyToken) -> FontId {
        let (selected, fallback, size) = match token {
            TypographyToken::DisplayBrand => {
                ("voidspace.unbounded.700", FontFamily::Proportional, 16.0)
            }
            TypographyToken::DisplayView => {
                ("voidspace.unbounded.600", FontFamily::Proportional, 15.0)
            }
            TypographyToken::DisplayAction => {
                ("voidspace.unbounded.700", FontFamily::Proportional, 10.0)
            }
            TypographyToken::UiTitle => ("voidspace.golos.600", FontFamily::Proportional, 16.0),
            TypographyToken::UiBody => ("voidspace.golos.400", FontFamily::Proportional, 13.0),
            TypographyToken::UiControl => ("voidspace.golos.500", FontFamily::Proportional, 13.0),
            TypographyToken::TileNameLarge => {
                ("voidspace.golos.500", FontFamily::Proportional, 11.0)
            }
            TypographyToken::TileNameCompact => {
                ("voidspace.golos.500", FontFamily::Proportional, 9.0)
            }
            TypographyToken::DataNormal => ("voidspace.jetbrains.500", FontFamily::Monospace, 12.0),
            TypographyToken::DataCompact => ("voidspace.jetbrains.500", FontFamily::Monospace, 9.0),
            TypographyToken::DataMicro => ("voidspace.jetbrains.500", FontFamily::Monospace, 9.0),
            TypographyToken::StatusLabel => ("voidspace.jetbrains.500", FontFamily::Monospace, 8.0),
            TypographyToken::StatusValue => {
                ("voidspace.jetbrains.500", FontFamily::Monospace, 11.0)
            }
        };
        FontId::new(
            size,
            if self.source == TypographySource::EmbeddedSelected {
                FontFamily::Name(selected.into())
            } else {
                fallback
            },
        )
    }
}

pub(crate) fn brand_wordmark(typography: &Typography) -> egui::text::LayoutJob {
    let mut wordmark = egui::text::LayoutJob::default();
    let font_id = typography.font(TypographyToken::DisplayBrand);
    wordmark.append(
        "VOID",
        0.0,
        egui::TextFormat {
            font_id: font_id.clone(),
            color: ORANGE,
            ..Default::default()
        },
    );
    wordmark.append(
        "SPACE",
        0.0,
        egui::TextFormat {
            font_id,
            color: TEXT,
            ..Default::default()
        },
    );
    wordmark
}

fn validate(bytes: &'static [u8], expected_sha: &str, weights: &[f32]) -> Result<(), String> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected_sha {
        return Err(format!("font hash mismatch: {actual}"));
    }
    let data = FontData::from_static(bytes);
    let axis = data
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag.to_be_bytes() == *b"wght")
        .ok_or_else(|| "missing wght axis".to_owned())?;
    if weights.iter().any(|weight| !axis.range.contains(*weight)) {
        return Err("wght range mismatch".into());
    }
    let font = FontRef::new(bytes).map_err(|error| error.to_string())?;
    if CYRILLIC_SENTINELS
        .chars()
        .filter(|character| !character.is_whitespace())
        .any(|character| font.charmap().map(character).is_none())
    {
        return Err("missing required Cyrillic glyph".into());
    }
    Ok(())
}

fn validate_selected_assets() -> Result<(), String> {
    validate(
        UNBOUNDED,
        "323b511be380c8d474ef030686b71aedde501f8d9cd46da558b7c40454372c3f",
        &[500.0, 600.0, 700.0],
    )?;
    validate(
        GOLOS,
        "17bb58fb69aec2dfb047a2ebf52534023e9b688c97a6b7ac795b0a72912c2063",
        &[400.0, 500.0, 600.0],
    )?;
    validate(
        JETBRAINS,
        "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
        &[400.0, 500.0, 600.0],
    )
}

pub fn release_typography_diagnostic() -> Result<String, String> {
    validate_selected_assets()?;
    Ok(concat!(
        "typography_source=embedded-selected ",
        "google_fonts_revision=6a003b5eb672dc8bf5bff5937cf5863f8b175445 ",
        "unbounded_sha256=323b511be380c8d474ef030686b71aedde501f8d9cd46da558b7c40454372c3f ",
        "golos_sha256=17bb58fb69aec2dfb047a2ebf52534023e9b688c97a6b7ac795b0a72912c2063 ",
        "jetbrains_sha256=48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda"
    )
    .to_owned())
}

fn register_weight(fonts: &mut FontDefinitions, family: &str, bytes: &'static [u8], weight: f32) {
    let fallback_family = if family.starts_with("voidspace.jetbrains") {
        FontFamily::Monospace
    } else {
        FontFamily::Proportional
    };
    let fallbacks = fonts
        .families
        .get(&fallback_family)
        .cloned()
        .unwrap_or_default();
    let data = FontData::from_static(bytes).tweak(FontTweak {
        coords: VariationCoords::new([(b"wght", weight)]),
        ..Default::default()
    });
    fonts.font_data.insert(family.into(), data.into());
    fonts.families.insert(
        FontFamily::Name(family.into()),
        std::iter::once(family.to_owned())
            .chain(fallbacks)
            .collect(),
    );
}

fn selected_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for weight in [500.0, 600.0, 700.0] {
        register_weight(
            &mut fonts,
            &format!("voidspace.unbounded.{weight:.0}"),
            UNBOUNDED,
            weight,
        );
    }
    for weight in [400.0, 500.0, 600.0] {
        register_weight(
            &mut fonts,
            &format!("voidspace.golos.{weight:.0}"),
            GOLOS,
            weight,
        );
        register_weight(
            &mut fonts,
            &format!("voidspace.jetbrains.{weight:.0}"),
            JETBRAINS,
            weight,
        );
    }
    fonts
}

pub fn install(context: &egui::Context) -> Typography {
    let source = match validate_selected_assets() {
        Ok(()) => {
            context.set_fonts(selected_definitions());
            TypographySource::EmbeddedSelected
        }
        Err(error) => {
            let _ =
                crate::diagnostics::log_line(&format!("embedded font validation failed: {error}"));
            context.set_fonts(FontDefinitions::default());
            TypographySource::EmbeddedFallback
        }
    };
    let typography = Typography {
        source,
        epoch: 1,
        pixels_per_point: context.pixels_per_point(),
    };
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
        typography.font(TypographyToken::UiTitle),
    );
    style
        .text_styles
        .insert(TextStyle::Body, typography.font(TypographyToken::UiBody));
    context.set_style_of(egui::Theme::Dark, style);
    context.set_theme(egui::Theme::Dark);
    typography
}

#[cfg(test)]
mod typography_tests {
    use super::*;

    #[test]
    fn approved_assets_match_hash_axis_and_cyrillic_contract() {
        validate_selected_assets().unwrap();
        assert!(
            release_typography_diagnostic()
                .unwrap()
                .starts_with("typography_source=embedded-selected")
        );
    }

    #[test]
    fn semantic_tokens_keep_the_approved_family_roles() {
        let selected = Typography::for_test(TypographySource::EmbeddedSelected, 1.0);
        assert_eq!(
            selected.font(TypographyToken::DisplayBrand).family,
            FontFamily::Name("voidspace.unbounded.700".into())
        );
        assert_eq!(
            selected.font(TypographyToken::TileNameLarge).family,
            FontFamily::Name("voidspace.golos.500".into())
        );
        assert_eq!(
            selected.font(TypographyToken::DataCompact).family,
            FontFamily::Name("voidspace.jetbrains.500".into())
        );
    }

    #[test]
    fn treemap_labels_use_the_compact_data_density() {
        let selected = Typography::for_test(TypographySource::EmbeddedSelected, 1.0);
        assert!(selected.font(TypographyToken::TileNameLarge).size <= 11.0);
        assert!(selected.font(TypographyToken::TileNameCompact).size <= 9.0);
        assert!(selected.font(TypographyToken::DataCompact).size <= 9.0);
    }

    #[test]
    fn status_values_use_a_readable_machine_typeface() {
        let selected = Typography::for_test(TypographySource::EmbeddedSelected, 1.0);
        let font = selected.font(TypographyToken::StatusValue);
        assert_eq!(
            font.family,
            FontFamily::Name("voidspace.jetbrains.500".into())
        );
        assert_eq!(font.size, 11.0);
    }

    #[test]
    fn dpi_change_advances_epoch_once() {
        let mut typography = Typography::for_test(TypographySource::EmbeddedSelected, 1.0);
        let initial = typography.epoch();
        assert!(!typography.update_pixels_per_point(1.0));
        assert!(typography.update_pixels_per_point(1.25));
        assert_eq!(typography.epoch(), initial + 1);
    }

    #[test]
    fn named_families_retain_offline_symbol_fallbacks() {
        let definitions = selected_definitions();
        for family in [
            "voidspace.unbounded.700",
            "voidspace.golos.500",
            "voidspace.jetbrains.500",
        ] {
            let chain = definitions
                .families
                .get(&FontFamily::Name(family.into()))
                .expect("registered named family");
            assert_eq!(chain.first().map(String::as_str), Some(family));
            assert!(chain.len() > 1, "{family} has no glyph fallback");
        }
    }

    #[test]
    fn wordmark_preserves_the_approved_two_color_split() {
        let typography = Typography::for_test(TypographySource::EmbeddedSelected, 1.0);
        let wordmark = brand_wordmark(&typography);

        assert_eq!(wordmark.text, "VOIDSPACE");
        assert_eq!(wordmark.sections.len(), 2);
        assert_eq!(wordmark.sections[0].byte_range.start.0, 0);
        assert_eq!(wordmark.sections[0].byte_range.end.0, 4);
        assert_eq!(wordmark.sections[0].format.color, ORANGE);
        assert_eq!(wordmark.sections[1].byte_range.start.0, 4);
        assert_eq!(wordmark.sections[1].byte_range.end.0, 9);
        assert_eq!(wordmark.sections[1].format.color, TEXT);
    }
}
