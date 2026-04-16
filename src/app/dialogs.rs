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
    let midi_connected = app.midi.input.is_connected();
    let midi_learning = app.midi.learning;
    let midi_learn_target = app.midi.learn_target;
    let pref_result = crate::ui::preferences::show_preferences(
        ctx,
        state,
        &app.available_plugins,
        &mut app.midi.map,
        midi_connected,
        midi_learning,
        midi_learn_target,
        &app.i18n,
    );
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
                app.status_message = app
                    .i18n
                    .trf("status.audio_restart_error", &[("error", &e.to_string())]);
                log::error!("Audio restart failed: {}", e);
            } else {
                app.status_message = app.i18n.trf(
                    "status.audio_info_short",
                    &[
                        ("sr", &(app.audio_engine.sample_rate as u32).to_string()),
                        ("buffer", &app.audio_engine.buffer_size.to_string()),
                    ],
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
                s.scan_status = app.i18n.trf(
                    "status.found_plugins",
                    &[("count", &app.available_plugins.len().to_string())],
                );
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
                app.status_message = app.i18n.trf(
                    "status.added_plugins_path",
                    &[("count", &new_count.to_string())],
                );
            } else {
                app.status_message = app.i18n.tr("status.no_plugins_path").into();
            }
            if let Some(ref mut s) = app.preferences_state {
                s.scan_status = app.i18n.trf(
                    "status.found_plugins",
                    &[("count", &app.available_plugins.len().to_string())],
                );
            }
        }
        PreferencesResult::SetInlineRackPluginGui(enabled) => {
            app.rack.inline_gui = enabled;
            app.close_all_rack_editors();
            app.status_message = if enabled {
                app.i18n.tr("status.rack_gui_inline").into()
            } else {
                app.i18n.tr("status.rack_gui_separate").into()
            };
        }
        PreferencesResult::SetLanguage(lang) => {
            app.set_language(lang);
        }
        PreferencesResult::MidiConnect(idx) => {
            let devices = crate::midi::MidiInput::enumerate_devices();
            if let Some(device) = devices.get(idx) {
                let device_name = device.name.clone();
                match app.midi.input.open_device(idx) {
                    Ok(()) => {
                        app.settings.midi_device_name = Some(device_name.clone());
                        app.settings_dirty = true;
                        app.status_message = app
                            .i18n
                            .trf("prefs.midi_connected", &[("name", &device_name)]);
                    }
                    Err(e) => {
                        app.status_message = app.i18n.trf("status.audio_error", &[("error", &e)]);
                    }
                }
            }
        }
        PreferencesResult::MidiDisconnect => {
            app.midi.input.close();
            app.settings.midi_device_name = None;
            app.settings_dirty = true;
        }
        PreferencesResult::MidiLearn(action) => {
            app.start_midi_learn(action);
        }
        PreferencesResult::MidiClearBinding(action) => {
            app.midi.map.remove_binding_for_action(action);
        }
        PreferencesResult::MidiSetTriggerMode(action, mode) => {
            if let Some(binding) = app.midi.map.find_binding(action) {
                let key = binding.key;
                app.midi.map.set_binding(key, action, mode);
            }
        }
        PreferencesResult::MidiClearAll => {
            app.midi.map.clear();
        }
        PreferencesResult::DisablePluginPath(path) => {
            if !app.disabled_plugin_paths.contains(&path) {
                app.disabled_plugin_paths.push(path);
            }
            app.scan_plugins();
        }
    }
}

pub(super) fn draw_about_dialog(app: &mut ToneDockApp, ctx: &Context) {
    if !app.show_about {
        return;
    }
    let mut open = app.show_about;
    Window::new(app.i18n.tr("dialog.about_title"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(app.i18n.tr("app.title"))
                        .size(24.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(app.i18n.tr("dialog.version"));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(app.i18n.tr("dialog.description"))
                        .size(11.0)
                        .color(crate::ui::theme::TEXT_SECONDARY),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(app.i18n.tr("dialog.license"))
                        .size(10.0)
                        .color(crate::ui::theme::TEXT_SECONDARY),
                );
                ui.add_space(12.0);
                ui.label(app.i18n.trf(
                    "dialog.audio_info",
                    &[
                        ("sr", &format!("{:.0}", app.audio_engine.sample_rate)),
                        ("buffer", &app.audio_engine.buffer_size.to_string()),
                    ],
                ));
                ui.add_space(4.0);
                ui.label(app.i18n.trf(
                    "dialog.plugins_scanned",
                    &[("count", &app.available_plugins.len().to_string())],
                ));
                ui.add_space(4.0);
                ui.label(app.i18n.trf(
                    "dialog.rack_slots",
                    &[("count", &app.audio_engine.chain_node_ids.len().to_string())],
                ));
            });
        });
    app.show_about = open;
}
