use egui::*;

pub const BG_SURFACE: Color32 = Color32::from_rgb(16, 16, 20);
pub const BG_PANEL: Color32 = Color32::from_rgb(24, 24, 30);
pub const BG_TEXTURE_TOP: Color32 = Color32::from_rgb(44, 40, 34);
pub const BG_TEXTURE_BOTTOM: Color32 = Color32::from_rgb(11, 11, 15);
#[allow(dead_code)]
pub const BG_CARD: Color32 = Color32::from_rgb(34, 34, 42);
#[allow(dead_code)]
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(42, 42, 52);
#[allow(dead_code)]
pub const BG_INPUT: Color32 = Color32::from_rgb(40, 40, 50);
pub const BG_SLOT: Color32 = Color32::from_rgb(38, 38, 48);
pub const BG_DARK: Color32 = Color32::from_rgb(12, 12, 16);

#[allow(dead_code)]
pub const PRIMARY: Color32 = Color32::from_rgb(208, 188, 255);
pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(79, 55, 139);
#[allow(dead_code)]
pub const ON_PRIMARY: Color32 = Color32::from_rgb(56, 30, 120);
pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(234, 221, 255);

#[allow(dead_code)]
pub const SECONDARY: Color32 = Color32::from_rgb(204, 194, 220);
#[allow(dead_code)]
pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(77, 68, 94);
#[allow(dead_code)]
pub const ON_SECONDARY: Color32 = Color32::from_rgb(50, 42, 66);
#[allow(dead_code)]
pub const ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(232, 222, 248);

#[allow(dead_code)]
pub const TERTIARY: Color32 = Color32::from_rgb(238, 184, 196);
#[allow(dead_code)]
pub const TERTIARY_CONTAINER: Color32 = Color32::from_rgb(99, 59, 72);
#[allow(dead_code)]
pub const ON_TERTIARY: Color32 = Color32::from_rgb(73, 37, 50);
#[allow(dead_code)]
pub const ON_TERTIARY_CONTAINER: Color32 = Color32::from_rgb(255, 216, 228);

pub const ACCENT: Color32 = Color32::from_rgb(208, 188, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(79, 55, 139);
pub const ACCENT_WARM: Color32 = Color32::from_rgb(214, 152, 74);

#[allow(dead_code)]
pub const SURFACE: Color32 = Color32::from_rgb(20, 20, 24);
#[allow(dead_code)]
pub const SURFACE_DIM: Color32 = Color32::from_rgb(14, 14, 18);
#[allow(dead_code)]
pub const SURFACE_BRIGHT: Color32 = Color32::from_rgb(54, 54, 60);
pub const SURFACE_CONTAINER_LOWEST: Color32 = Color32::from_rgb(10, 10, 14);
pub const SURFACE_CONTAINER_LOW: Color32 = Color32::from_rgb(22, 22, 28);
pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(26, 26, 32);
pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(36, 36, 42);
pub const SURFACE_CONTAINER_HIGHEST: Color32 = Color32::from_rgb(46, 46, 54);
#[allow(dead_code)]
pub const ON_SURFACE: Color32 = Color32::from_rgb(230, 224, 236);
#[allow(dead_code)]
pub const ON_SURFACE_VAR: Color32 = Color32::from_rgb(202, 196, 208);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 224, 236);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(202, 196, 208);
pub const TEXT_HINT: Color32 = Color32::from_rgb(150, 144, 158);

pub const OUTLINE: Color32 = Color32::from_rgb(78, 72, 86);
pub const OUTLINE_VAR: Color32 = Color32::from_rgb(54, 48, 66);
pub const DIVIDER: Color32 = Color32::from_rgb(42, 38, 52);

pub const KNOB_TRACK: Color32 = Color32::from_rgb(54, 48, 66);
#[allow(dead_code)]
pub const KNOB_FILL: Color32 = ACCENT;

pub const METER_GREEN: Color32 = Color32::from_rgb(102, 210, 120);
pub const METER_YELLOW: Color32 = Color32::from_rgb(240, 200, 60);
pub const METER_RED: Color32 = Color32::from_rgb(240, 90, 90);

pub const BYPASSED: Color32 = Color32::from_rgb(56, 30, 36);
pub const DISABLED: Color32 = Color32::from_rgb(30, 30, 36);

#[allow(dead_code)]
pub const ELEVATION_1: Color32 = Color32::from_black_alpha(20);
#[allow(dead_code)]
pub const ELEVATION_2: Color32 = Color32::from_black_alpha(36);
#[allow(dead_code)]
pub const ELEVATION_3: Color32 = Color32::from_black_alpha(52);

#[allow(dead_code)]
pub fn ripple_color() -> Color32 {
    Color32::from_rgba_unmultiplied(208, 188, 255, 30)
}

#[allow(dead_code)]
pub fn ripple_strong_color() -> Color32 {
    Color32::from_rgba_unmultiplied(208, 188, 255, 60)
}

pub fn apply_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    if let Ok(cjk_font) = load_system_cjk_font() {
        fonts.font_data.insert(
            "system-cjk".into(),
            std::sync::Arc::new(FontData::from_owned(cjk_font)),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .push("system-cjk".into());
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push("system-cjk".into());
    }

    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &Context) {
    let mut style = (*ctx.style()).clone();

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(16.0, 8.0);
    style.spacing.interact_size = Vec2::new(64.0, 36.0);
    style.spacing.combo_width = 200.0;

    let widget_bg = SURFACE_CONTAINER;
    let widget_bg_weak = SURFACE_CONTAINER_LOW;
    let widget_hover = SURFACE_CONTAINER_HIGHEST;

    use egui::style::{Selection, WidgetVisuals, Widgets};

    style.visuals = Visuals {
        dark_mode: true,
        override_text_color: Some(TEXT_PRIMARY),
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: widget_bg,
                weak_bg_fill: widget_bg_weak,
                bg_stroke: Stroke::new(1.0, OUTLINE_VAR),
                fg_stroke: Stroke::new(1.0, TEXT_PRIMARY),
                corner_radius: CornerRadius::same(12),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: widget_bg,
                weak_bg_fill: widget_bg_weak,
                bg_stroke: Stroke::new(1.0, OUTLINE_VAR),
                fg_stroke: Stroke::new(1.0, TEXT_PRIMARY),
                corner_radius: CornerRadius::same(12),
                expansion: 0.0,
            },
            hovered: WidgetVisuals {
                bg_fill: widget_hover,
                weak_bg_fill: SURFACE_CONTAINER_HIGH,
                bg_stroke: Stroke::new(1.0, OUTLINE),
                fg_stroke: Stroke::new(1.5, TEXT_PRIMARY),
                corner_radius: CornerRadius::same(12),
                expansion: 1.0,
            },
            active: WidgetVisuals {
                bg_fill: PRIMARY_CONTAINER,
                weak_bg_fill: PRIMARY_CONTAINER,
                bg_stroke: Stroke::new(1.5, ACCENT),
                fg_stroke: Stroke::new(2.0, ON_PRIMARY_CONTAINER),
                corner_radius: CornerRadius::same(12),
                expansion: 0.0,
            },
            open: WidgetVisuals {
                bg_fill: SURFACE_CONTAINER_HIGHEST,
                weak_bg_fill: SURFACE_CONTAINER_HIGH,
                bg_stroke: Stroke::new(1.0, OUTLINE),
                fg_stroke: Stroke::new(1.5, TEXT_PRIMARY),
                corner_radius: CornerRadius::same(12),
                expansion: 0.0,
            },
        },
        selection: Selection {
            bg_fill: PRIMARY_CONTAINER,
            stroke: Stroke::new(2.0, ACCENT),
        },
        hyperlink_color: ACCENT,
        faint_bg_color: SURFACE_CONTAINER_LOW,
        extreme_bg_color: BG_DARK,
        code_bg_color: Color32::from_rgb(20, 20, 26),
        warn_fg_color: METER_YELLOW,
        error_fg_color: METER_RED,
        window_corner_radius: CornerRadius::same(16),
        window_shadow: Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(100),
        },
        window_fill: SURFACE_CONTAINER,
        panel_fill: BG_SURFACE,
        ..Visuals::dark()
    };

    ctx.set_style(style);
}

pub fn paint_panel_texture(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, BG_TEXTURE_BOTTOM);

    let band_h = rect.height() * 0.18;
    painter.rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + band_h)),
        0.0,
        Color32::from_rgba_unmultiplied(
            BG_TEXTURE_TOP.r(),
            BG_TEXTURE_TOP.g(),
            BG_TEXTURE_TOP.b(),
            70,
        ),
    );

    let stripes = 28;
    for i in 0..stripes {
        let t = i as f32 / stripes as f32;
        let y = rect.top() + rect.height() * t;
        let alpha = if i % 2 == 0 { 14 } else { 7 };
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, alpha)),
        );
    }

    for i in 0..18 {
        let t = i as f32 / 17.0;
        let x = rect.left() + rect.width() * t;
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 5)),
        );
    }

    painter.rect_stroke(
        rect.shrink(0.5),
        CornerRadius::ZERO,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
        StrokeKind::Inside,
    );
}

pub fn paint_rack_bay(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::same(18), Color32::from_rgb(18, 18, 22));
    painter.rect_filled(
        rect.shrink(8.0),
        CornerRadius::same(14),
        Color32::from_rgb(8, 8, 10),
    );

    let rail_w = 26.0;
    for x in [rect.left() + 12.0, rect.right() - 12.0 - rail_w] {
        let rail = Rect::from_min_max(
            pos2(x, rect.top() + 18.0),
            pos2(x + rail_w, rect.bottom() - 18.0),
        );
        painter.rect_filled(rail, CornerRadius::same(12), Color32::from_rgb(22, 22, 26));
        for i in 0..9 {
            let cy = rail.top() + 24.0 + i as f32 * ((rail.height() - 48.0) / 8.0);
            painter.circle_filled(
                pos2(rail.center().x, cy),
                3.0,
                Color32::from_rgb(48, 48, 54),
            );
            painter.circle_filled(
                pos2(rail.center().x, cy) + vec2(-0.7, -0.7),
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 30),
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn load_system_cjk_font() -> Result<Vec<u8>, std::io::Error> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let candidates: Vec<std::path::PathBuf> = vec![
        std::path::Path::new(&windir)
            .join("Fonts")
            .join("YuGothR.ttc"),
        std::path::Path::new(&windir)
            .join("Fonts")
            .join("YuGothM.ttc"),
        std::path::Path::new(&windir)
            .join("Fonts")
            .join("msgothic.ttc"),
        std::path::Path::new(&windir)
            .join("Fonts")
            .join("meiryo.ttc"),
    ];
    for path in candidates {
        if path.exists() {
            if let Ok(data) = std::fs::read(&path) {
                return Ok(data);
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No CJK font found",
    ))
}

#[cfg(target_os = "macos")]
fn load_system_cjk_font() -> Result<Vec<u8>, std::io::Error> {
    let candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc"),
        std::path::PathBuf::from("/System/Library/Fonts/HiraginoSansGB.ttc"),
        std::path::PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        std::path::PathBuf::from("/Library/Fonts/Osaka.ttf"),
    ];
    for path in candidates {
        if path.exists() {
            if let Ok(data) = std::fs::read(&path) {
                return Ok(data);
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No CJK font found",
    ))
}

#[cfg(target_os = "linux")]
fn load_system_cjk_font() -> Result<Vec<u8>, std::io::Error> {
    let candidates: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        std::path::PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
        std::path::PathBuf::from("/usr/share/fonts/ipaex/ipaexg.ttf"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf"),
    ];
    for path in candidates {
        if path.exists() {
            if let Ok(data) = std::fs::read(&path) {
                return Ok(data);
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No CJK font found",
    ))
}
