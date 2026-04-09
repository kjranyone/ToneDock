use eframe::App;
use egui::*;

use super::{ToneDockApp, ViewMode};
use crate::audio::node::{LooperNodeState, MetronomeNodeState, NodeInternalState};
use crate::ui::preferences::PreferencesResult;

fn ui_section_frame() -> Frame {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(18, 18, 22, 210))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 12),
        ))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 6))
}

impl App for ToneDockApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        if self.main_hwnd.is_none() {
            if let Ok(hwnd) = crate::vst_host::editor::extract_hwnd_from_frame(frame) {
                self.main_hwnd = std::ptr::NonNull::new(hwnd);
            }
        }

        draw_toolbar(self, ctx);
        draw_transport(self, ctx);

        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(crate::ui::theme::BG_DARK)
                    .stroke(Stroke::NONE)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| match self.view_mode {
                ViewMode::Rack => self.show_rack_view(ui),
                ViewMode::NodeEditor => self.show_node_editor(ui),
            });

        handle_shortcuts(self, ctx);
        draw_preferences_dialog(self, ctx);
        draw_about_dialog(self, ctx);

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.audio_engine.stop();
    }
}

fn draw_toolbar(app: &mut ToneDockApp, ctx: &Context) {
    TopBottomPanel::top("toolbar")
        .exact_height(58.0)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            inner_margin: Margin::symmetric(10, 6),
            stroke: Stroke::NONE,
            ..Default::default()
        })
        .show(ctx, |ui| {
            let bar_rect = ui.max_rect();
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(34, 36, 39));
            ui.painter().rect_filled(
                Rect::from_min_max(bar_rect.min, pos2(bar_rect.max.x, bar_rect.min.y + 14.0)),
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(255, 255, 255, 14),
            );
            for i in 0..18 {
                let y = bar_rect.top() + i as f32 * 3.0;
                ui.painter().line_segment(
                    [pos2(bar_rect.left(), y), pos2(bar_rect.right(), y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 4)),
                );
            }
            ui.painter().line_segment(
                [
                    pos2(bar_rect.left(), bar_rect.bottom() - 1.0),
                    pos2(bar_rect.right(), bar_rect.bottom() - 1.0),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;

                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("ToneDock")
                            .size(19.0)
                            .color(crate::ui::theme::ACCENT)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Digital Guitar Rack")
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
                });

                ui.add_space(8.0);

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("FILE")
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        if ui.button("Save Preset").clicked() {
                            app.save_preset();
                        }
                        if ui.button("Load Preset").clicked() {
                            app.load_preset();
                        }
                        if ui.button("Import Session").clicked() {
                            app.import_session();
                        }
                        if ui.button("Settings").clicked() {
                            app.open_preferences();
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("ENGINE")
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let running = app.audio_engine.is_running();
                        let label = if running { "Stop Audio" } else { "Start Audio" };
                        if ui
                            .add_sized(
                                [92.0, 28.0],
                                Button::new(label).fill(if running {
                                    Color32::from_rgb(88, 42, 42)
                                } else {
                                    Color32::from_rgb(48, 78, 56)
                                }),
                            )
                            .clicked()
                        {
                            if running {
                                app.audio_engine.stop();
                            } else {
                                app.start_audio();
                            }
                        }

                        ui.label(
                            RichText::new("Master")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        let mut vol = app.master_volume;
                        ui.add_sized(
                            [88.0, 22.0],
                            egui::Slider::new(&mut vol, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        if (vol - app.master_volume).abs() > 0.001 {
                            app.master_volume = vol;
                            *app.audio_engine.master_volume.lock() = vol;
                        }

                        ui.label(
                            RichText::new("Gain")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        let mut gain = app.input_gain;
                        ui.add_sized(
                            [58.0, 24.0],
                            egui::DragValue::new(&mut gain).speed(0.01).range(0.0..=4.0),
                        );
                        if (gain - app.input_gain).abs() > 0.001 {
                            app.input_gain = gain;
                            *app.audio_engine.input_gain.lock() = gain;
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("VIEW")
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let view_label = match app.view_mode {
                            ViewMode::Rack => "Node View",
                            ViewMode::NodeEditor => "Rack View",
                        };
                        if ui.button(view_label).clicked() {
                            app.view_mode = match app.view_mode {
                                ViewMode::Rack => {
                                    if app.inline_rack_plugin_gui {
                                        app.close_all_rack_editors();
                                    }
                                    app.node_editor.set_selection(app.selected_rack_node);
                                    ViewMode::NodeEditor
                                }
                                ViewMode::NodeEditor => {
                                    let selection =
                                        app.node_editor.selected_node().filter(|node_id| {
                                            app.audio_engine.chain_node_ids.contains(node_id)
                                        });
                                    app.select_rack_plugin_node(selection);
                                    ViewMode::Rack
                                }
                            };
                        }
                        if ui.button("About").clicked() {
                            app.show_about = true;
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("EDIT")
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let can_undo = app.undo_manager.can_undo();
                        let can_redo = app.undo_manager.can_redo();
                        ui.add_enabled_ui(can_undo, |ui| {
                            if ui
                                .add_sized([42.0, 28.0], Button::new("\u{21a9}"))
                                .clicked()
                            {
                                app.perform_undo();
                            }
                        });
                        ui.add_enabled_ui(can_redo, |ui| {
                            if ui
                                .add_sized([42.0, 28.0], Button::new("\u{21aa}"))
                                .clicked()
                            {
                                app.perform_redo();
                            }
                        });
                    });
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui_section_frame().show(ui, |ui| {
                        ui.label(
                            RichText::new(&app.status_message)
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                    });
                });
            });
        });
}

fn draw_transport(app: &mut ToneDockApp, ctx: &Context) {
    TopBottomPanel::bottom("transport")
        .exact_height(56.0)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            inner_margin: Margin::symmetric(10, 6),
            stroke: Stroke::NONE,
            ..Default::default()
        })
        .show(ctx, |ui| {
            let bar_rect = ui.max_rect();
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(36, 38, 40));
            ui.painter().rect_filled(
                Rect::from_min_max(
                    pos2(bar_rect.left(), bar_rect.bottom() - 18.0),
                    bar_rect.max,
                ),
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(0, 0, 0, 44),
            );
            for i in 0..18 {
                let y = bar_rect.top() + i as f32 * 3.0;
                ui.painter().line_segment(
                    [pos2(bar_rect.left(), y), pos2(bar_rect.right(), y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 4)),
                );
            }
            ui.painter().line_segment(
                [
                    pos2(bar_rect.left(), bar_rect.top()),
                    pos2(bar_rect.right(), bar_rect.top()),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("METRONOME")
                                .size(9.0)
                                .color(crate::ui::theme::ACCENT),
                        );

                        if crate::ui::controls::draw_toggle(ui, "", app.metronome_enabled, 14.0) {
                            app.metronome_enabled = !app.metronome_enabled;
                            if app.metronome_node_id.is_none() && app.metronome_enabled {
                                app.metronome_node_id = Some(app.audio_engine.add_metronome_node());
                            }
                            if let Some(id) = app.metronome_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Metronome(MetronomeNodeState {
                                        bpm: app.metronome_bpm,
                                        volume: app.metronome_volume,
                                    }),
                                );
                                app.audio_engine
                                    .graph_set_enabled(id, app.metronome_enabled);
                                app.audio_engine.graph_commit_topology();
                            }
                        }

                        ui.label("BPM");
                        let mut bpm = app.metronome_bpm;
                        ui.add_sized(
                            [56.0, 24.0],
                            egui::DragValue::new(&mut bpm)
                                .speed(1.0)
                                .range(40.0..=300.0),
                        );
                        if (bpm - app.metronome_bpm).abs() > 0.01 {
                            app.metronome_bpm = bpm;
                            if let Some(id) = app.metronome_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Metronome(MetronomeNodeState {
                                        bpm,
                                        volume: app.metronome_volume,
                                    }),
                                );
                            }
                        }

                        ui.label("Vol");
                        let mut vol = app.metronome_volume;
                        ui.add_sized([74.0, 22.0], egui::Slider::new(&mut vol, 0.0..=1.0));
                        if (vol - app.metronome_volume).abs() > 0.001 {
                            app.metronome_volume = vol;
                            if let Some(id) = app.metronome_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Metronome(MetronomeNodeState {
                                        bpm: app.metronome_bpm,
                                        volume: vol,
                                    }),
                                );
                            }
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("LOOPER")
                                .size(10.0)
                                .color(crate::ui::theme::ACCENT),
                        );

                        if crate::ui::controls::draw_toggle(ui, "", app.looper_enabled, 14.0) {
                            app.looper_enabled = !app.looper_enabled;
                            if app.looper_node_id.is_none() && app.looper_enabled {
                                app.looper_node_id = Some(app.audio_engine.add_looper_node());
                            }
                            if let Some(id) = app.looper_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Looper(LooperNodeState {
                                        enabled: app.looper_enabled,
                                        recording: false,
                                        playing: false,
                                        overdubbing: false,
                                        cleared: false,
                                    }),
                                );
                                app.audio_engine.graph_set_enabled(id, app.looper_enabled);
                                app.audio_engine.graph_commit_topology();
                            }
                            if !app.looper_enabled {
                                app.looper_recording = false;
                                app.looper_playing = false;
                                app.looper_overdubbing = false;
                                if let Some(id) = app.looper_node_id {
                                    app.audio_engine.graph_set_state(
                                        id,
                                        NodeInternalState::Looper(LooperNodeState {
                                            enabled: false,
                                            recording: false,
                                            playing: false,
                                            overdubbing: false,
                                            cleared: true,
                                        }),
                                    );
                                }
                            }
                        }

                        let rec_fill = if app.looper_recording {
                            Color32::from_rgb(112, 40, 40)
                        } else {
                            crate::ui::theme::SURFACE_CONTAINER_HIGH
                        };
                        if ui
                            .add_sized([48.0, 28.0], Button::new("Rec").fill(rec_fill))
                            .clicked()
                        {
                            if app.looper_node_id.is_none() {
                                app.looper_node_id = Some(app.audio_engine.add_looper_node());
                            }
                            app.looper_enabled = true;
                            app.looper_recording = !app.looper_recording;
                            app.looper_playing = !app.looper_recording;
                            if let Some(id) = app.looper_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Looper(LooperNodeState {
                                        enabled: true,
                                        recording: app.looper_recording,
                                        playing: app.looper_playing,
                                        overdubbing: false,
                                        cleared: false,
                                    }),
                                );
                                app.audio_engine.graph_set_enabled(id, true);
                                app.audio_engine.graph_commit_topology();
                            }
                        }

                        let play_fill = if app.looper_playing {
                            Color32::from_rgb(56, 80, 62)
                        } else {
                            crate::ui::theme::SURFACE_CONTAINER_HIGH
                        };
                        if ui
                            .add_sized([50.0, 28.0], Button::new("Play").fill(play_fill))
                            .clicked()
                        {
                            if app.looper_node_id.is_none() {
                                app.looper_node_id = Some(app.audio_engine.add_looper_node());
                            }
                            app.looper_playing = !app.looper_playing;
                            app.looper_recording = false;
                            if let Some(id) = app.looper_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Looper(LooperNodeState {
                                        enabled: true,
                                        recording: false,
                                        playing: app.looper_playing,
                                        overdubbing: app.looper_overdubbing,
                                        cleared: false,
                                    }),
                                );
                                app.audio_engine.graph_set_enabled(id, true);
                                app.audio_engine.graph_commit_topology();
                            }
                        }

                        let dub_fill = if app.looper_overdubbing {
                            Color32::from_rgb(88, 72, 30)
                        } else {
                            crate::ui::theme::SURFACE_CONTAINER_HIGH
                        };
                        if ui
                            .add_sized([68.0, 28.0], Button::new("Overdub").fill(dub_fill))
                            .clicked()
                        {
                            app.looper_overdubbing = if app.looper_overdubbing {
                                false
                            } else if app.looper_playing {
                                true
                            } else {
                                false
                            };
                            if let Some(id) = app.looper_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Looper(LooperNodeState {
                                        enabled: true,
                                        recording: app.looper_recording,
                                        playing: app.looper_playing,
                                        overdubbing: app.looper_overdubbing,
                                        cleared: false,
                                    }),
                                );
                            }
                        }

                        if ui.add_sized([52.0, 28.0], Button::new("Clear")).clicked() {
                            app.looper_recording = false;
                            app.looper_playing = false;
                            app.looper_overdubbing = false;
                            if let Some(id) = app.looper_node_id {
                                app.audio_engine.graph_set_state(
                                    id,
                                    NodeInternalState::Looper(LooperNodeState {
                                        enabled: false,
                                        recording: false,
                                        playing: false,
                                        overdubbing: false,
                                        cleared: true,
                                    }),
                                );
                            }
                        }
                    });
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui_section_frame().show(ui, |ui| {
                        if let Some(id) = app.looper_node_id {
                            let guard = app.audio_engine.graph.load();
                            let loop_samples = guard.looper_loop_length(id);
                            drop(guard);
                            if loop_samples > 0 {
                                let sr = app.audio_engine.sample_rate;
                                let secs = loop_samples as f64 / sr;
                                ui.label(
                                    RichText::new(format!("Loop {:.1}s", secs))
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_SECONDARY),
                                );
                            } else {
                                ui.label(
                                    RichText::new("Transport idle")
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_HINT),
                                );
                            }
                        } else {
                            ui.label(
                                RichText::new("Transport idle")
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                        }
                    });
                });
            });
        });
}

fn handle_shortcuts(app: &mut ToneDockApp, ctx: &Context) {
    if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
        app.perform_undo();
    }
    if ctx.input(|i| {
        (i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
            || (i.key_pressed(egui::Key::Y) && i.modifiers.ctrl)
    }) {
        app.perform_redo();
    }
}

fn draw_preferences_dialog(app: &mut ToneDockApp, ctx: &Context) {
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

fn draw_about_dialog(app: &mut ToneDockApp, ctx: &Context) {
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
