pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(30, 30, 35);
pub const BG_PANEL: egui::Color32 = egui::Color32::from_rgb(40, 40, 48);
pub const BG_SLOT: egui::Color32 = egui::Color32::from_rgb(50, 50, 60);
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 180, 216);
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(0, 120, 144);
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(230, 230, 235);
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(160, 160, 170);
pub const KNOB_TRACK: egui::Color32 = egui::Color32::from_rgb(80, 80, 90);
pub const METER_GREEN: egui::Color32 = egui::Color32::from_rgb(0, 200, 80);
pub const METER_YELLOW: egui::Color32 = egui::Color32::from_rgb(255, 200, 0);
pub const METER_RED: egui::Color32 = egui::Color32::from_rgb(255, 60, 60);
pub const BYPASSED: egui::Color32 = egui::Color32::from_rgb(120, 60, 60);
pub const DISABLED: egui::Color32 = egui::Color32::from_rgb(60, 60, 60);

pub fn apply_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Ok(cjk_font) = load_system_cjk_font() {
        fonts.font_data.insert(
            "system-cjk".into(),
            std::sync::Arc::new(egui::FontData::from_owned(cjk_font)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("system-cjk".into());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("system-cjk".into());
    }

    ctx.set_fonts(fonts);
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
