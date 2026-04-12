mod audio_tab;
mod plugins_tab;

use egui::*;

use crate::i18n::{I18n, Language};
use crate::vst_host::scanner::PluginInfo;

pub use audio_tab::AudioSettingsState;

pub enum PreferencesTab {
    Audio,
    Plugins,
}

pub struct PreferencesState {
    pub tab: PreferencesTab,
    pub audio: AudioSettingsState,
    pub custom_plugin_paths: Vec<std::path::PathBuf>,
    pub scan_status: String,
    pub inline_rack_plugin_gui: bool,
}

pub enum PreferencesResult {
    None,
    AudioApply {
        host_id: Option<cpal::HostId>,
        input_name: Option<String>,
        output_name: Option<String>,
        sample_rate: u32,
        buffer_size: u32,
        input_ch: usize,
        output_ch: (usize, usize),
    },
    AudioCancel,
    RescanPlugins,
    AddPluginPath(std::path::PathBuf),
    SetInlineRackPluginGui(bool),
    SetLanguage(Language),
}

impl PreferencesState {
    pub fn new(
        current_host: Option<cpal::HostId>,
        current_sr: u32,
        current_bs: u32,
        custom_plugin_paths: Vec<std::path::PathBuf>,
        current_input_ch: usize,
        current_output_ch: (usize, usize),
        inline_rack_plugin_gui: bool,
    ) -> Self {
        Self {
            tab: PreferencesTab::Audio,
            audio: AudioSettingsState::new(
                current_host,
                current_sr,
                current_bs,
                current_input_ch,
                current_output_ch,
            ),
            custom_plugin_paths,
            scan_status: String::new(),
            inline_rack_plugin_gui,
        }
    }
}

pub(super) const SZ_SECTION: f32 = 13.0;
pub(super) const SZ_BODY: f32 = 13.0;
pub(super) const SZ_SMALL: f32 = 11.0;
pub(super) const SZ_PATH: f32 = 12.0;

fn preferences_window_frame() -> Frame {
    Frame::new()
        .fill(crate::ui::theme::SURFACE_CONTAINER_HIGH)
        .stroke(Stroke::new(1.0, crate::ui::theme::OUTLINE_VAR))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::symmetric(14, 12))
}

fn preferences_panel_frame() -> Frame {
    Frame::new()
        .fill(crate::ui::theme::BG_PANEL)
        .stroke(Stroke::new(1.0, crate::ui::theme::OUTLINE_VAR))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(14, 12))
}

pub fn show_preferences(
    ctx: &Context,
    state: &mut PreferencesState,
    available_plugins: &[PluginInfo],
    i18n: &I18n,
) -> PreferencesResult {
    let mut result = PreferencesResult::None;

    Window::new(i18n.tr("prefs.title"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(true)
        .collapsible(false)
        .default_size([560.0, 560.0])
        .min_size([440.0, 380.0])
        .frame(preferences_window_frame())
        .show(ctx, |ui| {
            preferences_panel_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let audio_selected = matches!(state.tab, PreferencesTab::Audio);
                    let plugins_selected = matches!(state.tab, PreferencesTab::Plugins);

                    if ui
                        .selectable_label(audio_selected, i18n.tr("prefs.audio"))
                        .clicked()
                    {
                        state.tab = PreferencesTab::Audio;
                    }
                    if ui
                        .selectable_label(plugins_selected, i18n.tr("prefs.plugins_vst"))
                        .clicked()
                    {
                        state.tab = PreferencesTab::Plugins;
                    }
                });

                ui.separator();
                ui.add_space(4.0);

                ui.label(
                    RichText::new(i18n.tr("prefs.language"))
                        .size(SZ_SECTION)
                        .color(crate::ui::theme::ACCENT),
                );
                ui.add_space(2.0);

                let current_lang = i18n.language();
                egui::ComboBox::from_id_salt("language_select")
                    .selected_text(current_lang.display_name())
                    .show_ui(ui, |ui| {
                        for lang in Language::ALL {
                            if ui
                                .selectable_label(current_lang == lang, lang.display_name())
                                .clicked()
                            {
                                result = PreferencesResult::SetLanguage(lang);
                            }
                        }
                    });

                ui.add_space(8.0);

                match state.tab {
                    PreferencesTab::Audio => {
                        let audio_result = audio_tab::show_audio_tab(ui, &mut state.audio, i18n);
                        if matches!(result, PreferencesResult::None) {
                            result = audio_result;
                        }
                    }
                    PreferencesTab::Plugins => {
                        let plugins_result =
                            plugins_tab::show_plugins_tab(ui, state, available_plugins, i18n);
                        if matches!(result, PreferencesResult::None) {
                            result = plugins_result;
                        }
                    }
                }
            });
        });

    result
}
