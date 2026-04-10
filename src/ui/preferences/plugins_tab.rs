use egui::*;

use crate::vst_host::scanner::PluginInfo;

use super::{PreferencesResult, PreferencesState, SZ_BODY, SZ_PATH, SZ_SECTION, SZ_SMALL};

pub(super) fn show_plugins_tab(
    ui: &mut Ui,
    state: &mut PreferencesState,
    available_plugins: &[PluginInfo],
) -> PreferencesResult {
    let mut result = PreferencesResult::None;

    let before_inline_mode = state.inline_rack_plugin_gui;

    ui.label(
        RichText::new("RACK GUI MODE")
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(4.0);
    ui.checkbox(
        &mut state.inline_rack_plugin_gui,
        "Inline plugin GUI inside Rack Mode",
    );
    ui.label(
        RichText::new(
            "Off: open VST GUI in a separate window. On: show the selected plugin GUI inline in the rack area.",
        )
        .size(SZ_SMALL)
        .color(crate::ui::theme::TEXT_SECONDARY),
    );
    ui.add_space(10.0);

    if state.inline_rack_plugin_gui != before_inline_mode {
        result = PreferencesResult::SetInlineRackPluginGui(state.inline_rack_plugin_gui);
    }

    ui.horizontal(|ui| {
        if ui.button("Rescan All Plugins").clicked() {
            result = PreferencesResult::RescanPlugins;
        }

        if ui.button("Add Plugin Path...").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Select VST3 Plugin Folder")
                .pick_folder()
            {
                result = PreferencesResult::AddPluginPath(path);
            }
        }
    });

    if !state.scan_status.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(&state.scan_status)
                .size(SZ_BODY)
                .color(crate::ui::theme::TEXT_SECONDARY),
        );
    }

    ui.add_space(6.0);

    ui.label(
        RichText::new("PLUGIN SEARCH PATHS")
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(4.0);

    let default_paths = crate::vst_host::scanner::PluginScanner::default_vst3_paths();

    for path in &default_paths {
        let exists = path.exists();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("\u{25cf}")
                    .size(SZ_SMALL)
                    .color(crate::ui::theme::ACCENT_DIM),
            );
            ui.label(
                RichText::new(path.to_string_lossy())
                    .size(SZ_PATH)
                    .color(if exists {
                        crate::ui::theme::TEXT_SECONDARY
                    } else {
                        crate::ui::theme::DISABLED
                    }),
            );
            if !exists {
                ui.label(
                    RichText::new("(not found)")
                        .size(SZ_SMALL)
                        .color(crate::ui::theme::DISABLED),
                );
            }
        });
    }

    if !state.custom_plugin_paths.is_empty() {
        ui.add_space(2.0);
        let mut remove_idx = None;
        for (i, path) in state.custom_plugin_paths.iter().enumerate() {
            let exists = path.exists();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("+")
                        .size(SZ_SMALL)
                        .color(crate::ui::theme::METER_GREEN),
                );
                ui.label(
                    RichText::new(path.to_string_lossy())
                        .size(SZ_PATH)
                        .color(if exists {
                            crate::ui::theme::TEXT_SECONDARY
                        } else {
                            crate::ui::theme::DISABLED
                        }),
                );
                if !exists {
                    ui.label(
                        RichText::new("(not found)")
                            .size(SZ_SMALL)
                            .color(crate::ui::theme::DISABLED),
                    );
                }
                if ui.small_button("Remove").clicked() {
                    remove_idx = Some(i);
                }
            });
        }
        if let Some(idx) = remove_idx {
            state.custom_plugin_paths.remove(idx);
        }
    }

    ui.add_space(10.0);

    ui.label(
        RichText::new(format!("DISCOVERED PLUGINS ({})", available_plugins.len()))
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(4.0);

    let available_height = ui.available_height();
    egui::ScrollArea::vertical()
        .max_height(available_height)
        .show(ui, |ui| {
            if available_plugins.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("No plugins found")
                            .size(SZ_BODY)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                    ui.label(
                        RichText::new("Click 'Rescan All Plugins' or add a custom path")
                            .size(SZ_SMALL)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                });
            } else {
                for plugin in available_plugins {
                    Frame::group(ui.style())
                        .fill(crate::ui::theme::BG_SLOT)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&plugin.name)
                                            .size(SZ_BODY)
                                            .color(crate::ui::theme::TEXT_PRIMARY),
                                    );
                                    if !plugin.vendor.is_empty() || !plugin.category.is_empty() {
                                        let detail = if !plugin.vendor.is_empty()
                                            && !plugin.category.is_empty()
                                        {
                                            format!("{} / {}", plugin.vendor, plugin.category)
                                        } else {
                                            format!("{}{}", plugin.vendor, plugin.category)
                                        };
                                        ui.label(
                                            RichText::new(detail)
                                                .size(SZ_SMALL)
                                                .color(crate::ui::theme::TEXT_SECONDARY),
                                        );
                                    }
                                    ui.label(
                                        RichText::new(plugin.path.to_string_lossy())
                                            .size(SZ_SMALL)
                                            .color(crate::ui::theme::TEXT_SECONDARY),
                                    );
                                });
                            });
                        });
                    ui.add_space(2.0);
                }
            }
        });

    result
}
