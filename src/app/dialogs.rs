use egui::*;

use super::ToneDockApp;
use crate::ui::preferences::PreferencesResult;

pub(super) fn draw_preferences_dialog(app: &mut ToneDockApp, ctx: &Context) {
    if !app.show_preferences {
        return;
    }
    let Some(ref mut state) = app.preferences_state else {
        return;
    };
    let pref_result = crate::ui::preferences::show_preferences(ctx, state, &app.available_plugins);
    match pref_result {
        PreferencesResult::None => {}
        PreferencesResult::AudioApply {
            host_id,
            input_name,
            output_name,
            sample_rate,
            buffer_size,
            input_ch,
            output_ch,
        } => {
            app.show_preferences = false;
            if let Err(e) = app.audio_engine.restart_with_config(
                host_id,
                input_name.as_deref(),
                output_name.as_deref(),
                sample_rate,
                buffer_size,
                input_ch,
                output_ch,
            ) {
                app.status_message = format!("Audio restart error: {}", e);
                log::error!("Audio restart failed: {}", e);
            } else {
                app.status_message = format!(
                    "Audio: {}Hz, buffer {}",
                    app.audio_engine.sample_rate as u32, app.audio_engine.buffer_size,
                );
            }
            app.preferences_state = None;
        }
        PreferencesResult::AudioCancel => {
            app.show_preferences = false;
            app.preferences_state = None;
        }
        PreferencesResult::RescanPlugins => {
            app.custom_plugin_paths = state.custom_plugin_paths.clone();
            app.scan_plugins_with_custom_paths();
            if let Some(ref mut s) = app.preferences_state {
                s.scan_status = format!("Found {} plugins", app.available_plugins.len());
            }
        }
        PreferencesResult::AddPluginPath(path) => {
            if !app.custom_plugin_paths.contains(&path) {
                app.custom_plugin_paths.push(path.clone());
            }
            if let Some(ref mut s) = app.preferences_state {
                s.custom_plugin_paths = app.custom_plugin_paths.clone();
            }
            let mut scanner = crate::vst_host::scanner::PluginScanner::new();
            scanner.add_path(path);
            let plugins = scanner.scan();
            if !plugins.is_empty() {
                let mut seen: std::collections::HashSet<std::path::PathBuf> = app
                    .available_plugins
                    .iter()
                    .map(|p| p.path.clone())
                    .collect();
                let new_count = plugins.len();
                for p in plugins {
                    if seen.insert(p.path.clone()) {
                        app.available_plugins.push(p);
                    }
                }
                app.status_message = format!("Added {} plugins from custom path", new_count);
            } else {
                app.status_message = "No plugins found in selected path".into();
            }
            if let Some(ref mut s) = app.preferences_state {
                s.scan_status = format!("Found {} plugins", app.available_plugins.len());
            }
        }
        PreferencesResult::SetInlineRackPluginGui(enabled) => {
            app.inline_rack_plugin_gui = enabled;
            app.close_all_rack_editors();
            app.status_message = if enabled {
                "Rack GUI mode: inline".into()
            } else {
                "Rack GUI mode: separate window".into()
            };
        }
    }
}

pub(super) fn draw_about_dialog(app: &mut ToneDockApp, ctx: &Context) {
    if !app.show_about {
        return;
    }
    let mut open = app.show_about;
    Window::new("About ToneDock")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("ToneDock")
                        .size(24.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label("v0.1.0");
                ui.add_space(4.0);
                ui.label(
                    RichText::new("A guitar practice VST3 host application")
                        .size(11.0)
                        .color(crate::ui::theme::TEXT_SECONDARY),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("GPL-3.0 License")
                        .size(10.0)
                        .color(crate::ui::theme::TEXT_SECONDARY),
                );
                ui.add_space(12.0);
                ui.label(format!(
                    "Audio: {:.0} Hz / {} buffer",
                    app.audio_engine.sample_rate, app.audio_engine.buffer_size
                ));
                ui.add_space(4.0);
                ui.label(format!("Plugins scanned: {}", app.available_plugins.len()));
                ui.add_space(4.0);
                ui.label(format!(
                    "Rack slots: {}",
                    app.audio_engine.chain_node_ids.len()
                ));
            });
        });
    app.show_about = open;
}
