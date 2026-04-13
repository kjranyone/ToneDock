use eframe::App;
use egui::*;

use super::{ToneDockApp, ViewMode};
use crate::audio::node::NodeInternalState;

pub(super) fn ui_section_frame() -> Frame {
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
        super::transport::draw_transport(self, ctx);

        self.poll_midi();
        self.poll_scan_results();

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
        super::dialogs::draw_preferences_dialog(self, ctx);
        super::dialogs::draw_about_dialog(self, ctx);

        if let Some(storage) = frame.storage_mut() {
            self.sync_settings_from_engine();
            self.save_settings(storage);
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.autosave();
        self.audio_engine.stop();
    }

    fn persist_egui_memory(&self) -> bool {
        true
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
                        RichText::new(app.i18n.tr("app.title"))
                            .size(19.0)
                            .color(crate::ui::theme::ACCENT)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(app.i18n.tr("app.subtitle"))
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
                });

                ui.add_space(8.0);

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.file"))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        if ui.button(app.i18n.tr("toolbar.save_preset")).clicked() {
                            app.save_preset();
                        }
                        if ui.button(app.i18n.tr("toolbar.load_preset")).clicked() {
                            app.load_preset();
                        }
                        if ui.button(app.i18n.tr("toolbar.import_session")).clicked() {
                            app.import_session();
                        }
                        if ui.button(app.i18n.tr("toolbar.save_workspace")).clicked() {
                            app.save_workspace();
                        }
                        if ui.button(app.i18n.tr("toolbar.load_workspace")).clicked() {
                            app.load_workspace();
                        }

                        ui.separator();

                        if ui.button(app.i18n.tr("toolbar.ab_snap_a")).clicked() {
                            app.snapshot_to_ab('a');
                        }
                        if ui.button(app.i18n.tr("toolbar.ab_snap_b")).clicked() {
                            app.snapshot_to_ab('b');
                        }
                        if ui.button(app.i18n.tr("toolbar.ab_restore_a")).clicked() {
                            app.restore_ab('a');
                        }
                        if ui.button(app.i18n.tr("toolbar.ab_restore_b")).clicked() {
                            app.restore_ab('b');
                        }

                        let suggestions = app.suggest_plugin_chain();
                        if !suggestions.is_empty() {
                            ui.separator();
                            ui.menu_button(app.i18n.tr("toolbar.ai_suggest"), |ui| {
                                for suggestion in &suggestions {
                                    ui.label(RichText::new(suggestion).size(9.0));
                                }
                            });
                        }
                        if ui.button(app.i18n.tr("toolbar.settings")).clicked() {
                            app.open_preferences();
                        }

                        let menu_label = app.i18n.tr("toolbar.practice_templates").to_owned();
                        let template_labels: Vec<(String, String)> =
                            crate::app::ToneDockApp::practice_template_names()
                                .iter()
                                .map(|(key, label_key)| {
                                    (key.to_string(), app.i18n.tr(label_key).to_owned())
                                })
                                .collect();
                        ui.menu_button(menu_label, |ui| {
                            for (key, label) in &template_labels {
                                let key = key.clone();
                                if ui.button(label.as_str()).clicked() {
                                    app.apply_practice_template(&key);
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.engine"))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let running = app.audio_engine.is_running();
                        let label = if running {
                            app.i18n.tr("toolbar.stop_audio")
                        } else {
                            app.i18n.tr("toolbar.start_audio")
                        };
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
                            RichText::new(app.i18n.tr("toolbar.master"))
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
                            RichText::new(app.i18n.tr("toolbar.gain"))
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

                        if ui
                            .add_sized([52.0, 24.0], Button::new(app.i18n.tr("toolbar.tap_tempo")))
                            .clicked()
                        {
                            app.tap_tempo();
                        }

                        let timer_label = if app.practice_timer_start.is_some() {
                            app.i18n.tr("toolbar.timer_stop")
                        } else {
                            app.i18n.tr("toolbar.timer_start")
                        };
                        if ui
                            .add_sized([52.0, 24.0], Button::new(timer_label))
                            .clicked()
                        {
                            if app.practice_timer_start.is_some() {
                                if let Some(start) = app.practice_timer_start {
                                    app.settings.total_practice_secs += start.elapsed().as_secs();
                                    app.settings_dirty = true;
                                }
                                app.practice_timer_start = None;
                            } else {
                                app.practice_timer_start = Some(std::time::Instant::now());
                            }
                        }

                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.bpm_goal"))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let mut bpm_goal = app.settings.bpm_goal.unwrap_or(0.0);
                        ui.add_sized(
                            [42.0, 18.0],
                            egui::DragValue::new(&mut bpm_goal)
                                .speed(1.0)
                                .range(0.0..=300.0),
                        );
                        let new_goal = if bpm_goal < 1.0 { None } else { Some(bpm_goal) };
                        if new_goal != app.settings.bpm_goal {
                            app.settings.bpm_goal = new_goal;
                            app.settings_dirty = true;
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.recorder"))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        if app.recorder_node_id.is_none() {
                            if ui.button(app.i18n.tr("toolbar.add_recorder")).clicked() {
                                app.recorder_node_id = Some(app.audio_engine.add_recorder_node());
                            }
                        } else {
                            let is_recording = if let Some(id) = app.recorder_node_id {
                                let guard = app.audio_engine.graph.load();
                                guard.get_node(id).map_or(false, |n| {
                                    matches!(&n.internal_state, crate::audio::node::NodeInternalState::Recorder(s) if s.recording)
                                })
                            } else {
                                false
                            };
                            let rec_fill = if is_recording {
                                Color32::from_rgb(140, 30, 30)
                            } else {
                                crate::ui::theme::SURFACE_CONTAINER_HIGH
                            };
                            let rec_label = if is_recording {
                                app.i18n.tr("toolbar.stop_rec")
                            } else {
                                app.i18n.tr("toolbar.start_rec")
                            };
                            if ui.add_sized([60.0, 24.0], Button::new(rec_label).fill(rec_fill)).clicked() {
                                if let Some(id) = app.recorder_node_id {
                                    if is_recording {
                                        app.audio_engine.stop_recorder(id);
                                        app.status_message = app.i18n.tr("status.recording_stopped").into();
                                    } else {
                                        app.audio_engine.start_recorder(id, 2);
                                        app.status_message = app.i18n.tr("status.recording_started").into();
                                    }
                                }
                            }
                            if ui.button(app.i18n.tr("toolbar.export_rec")).clicked() {
                                if let Some(id) = app.recorder_node_id {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("WAV".to_string(), &["wav"])
                                        .save_file()
                                    {
                                        match app.audio_engine.export_recorder_wav(id, &path) {
                                            Ok(()) => {
                                                app.status_message = app.i18n.tr("status.recording_exported").into();
                                            }
                                            Err(e) => {
                                                app.status_message = app.i18n.trf(
                                                    "status.recording_export_error",
                                                    &[("error", &e.to_string())],
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.view"))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                        let view_label = match app.view_mode {
                            ViewMode::Rack => app.i18n.tr("toolbar.node_view"),
                            ViewMode::NodeEditor => app.i18n.tr("toolbar.rack_view"),
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
                        if ui.button(app.i18n.tr("toolbar.about")).clicked() {
                            app.show_about = true;
                        }
                        if ui.button(app.i18n.tr("toolbar.fullscreen")).clicked() {
                            app.fullscreen = !app.fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                app.fullscreen,
                            ));
                        }
                    });
                });

                ui_section_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.edit"))
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
                        ui.horizontal(|ui| {
                            let dropouts = *app.audio_engine.dropout_count.lock();
                            if dropouts > app.last_dropout_count {
                                app.status_message = app.i18n.trf(
                                    "status.dropout_detected",
                                    &[("count", &dropouts.to_string())],
                                );
                                app.last_dropout_count = dropouts;
                            }
                            let dropout_color = if dropouts > 0 {
                                crate::ui::theme::METER_YELLOW
                            } else {
                                crate::ui::theme::TEXT_HINT
                            };
                            ui.label(
                                RichText::new(
                                    app.i18n.trf(
                                        "toolbar.dropouts",
                                        &[("count", &dropouts.to_string())],
                                    ),
                                )
                                .size(9.0)
                                .color(dropout_color),
                            );

                            let cpu = *app.audio_engine.cpu_usage.lock();
                            let cpu_color = if cpu > 80.0 {
                                crate::ui::theme::METER_RED
                            } else if cpu > 50.0 {
                                crate::ui::theme::METER_YELLOW
                            } else {
                                crate::ui::theme::METER_GREEN
                            };
                            ui.label(
                                RichText::new(
                                    app.i18n
                                        .trf("toolbar.cpu", &[("percent", &format!("{:.0}", cpu))]),
                                )
                                .size(9.0)
                                .color(cpu_color),
                            );

                            let latency_ms = app.audio_engine.buffer_size as f64
                                / app.audio_engine.sample_rate
                                * 1000.0;
                            ui.label(
                                RichText::new(app.i18n.trf(
                                    "toolbar.latency",
                                    &[("ms", &format!("{:.1}", latency_ms))],
                                ))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_HINT),
                            );

                            if let Some(start) = app.practice_timer_start {
                                let elapsed = start.elapsed();
                                let secs = elapsed.as_secs();
                                let mins = secs / 60;
                                let secs = secs % 60;
                                ui.label(
                                    RichText::new(app.i18n.trf(
                                        "toolbar.timer",
                                        &[
                                            ("min", &mins.to_string()),
                                            ("sec", &format!("{:02}", secs)),
                                        ],
                                    ))
                                    .size(9.0)
                                    .color(crate::ui::theme::ACCENT_WARM),
                                );
                            }

                            let total = app.settings.total_practice_secs
                                + app
                                    .practice_timer_start
                                    .map(|s| s.elapsed().as_secs())
                                    .unwrap_or(0);
                            if total > 0 {
                                let total_h = total / 3600;
                                let total_m = (total % 3600) / 60;
                                ui.label(
                                    RichText::new(app.i18n.trf(
                                        "toolbar.total_practice",
                                        &[
                                            ("hours", &total_h.to_string()),
                                            ("mins", &format!("{:02}", total_m)),
                                        ],
                                    ))
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                                );
                            }

                            if let Some(goal) = app.settings.bpm_goal {
                                if goal > 0.0 && app.metronome_bpm >= goal {
                                    ui.label(
                                        RichText::new(app.i18n.tr("toolbar.goal_reached"))
                                            .size(9.0)
                                            .color(crate::ui::theme::METER_GREEN),
                                    );
                                }
                            }

                            if app.scanning_in_progress {
                                ui.spinner();
                            }

                            ui.label(
                                RichText::new(&app.status_message)
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        });
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
    if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
        app.fullscreen = !app.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(app.fullscreen));
    }
    if ctx.input(|i| i.key_pressed(egui::Key::T) && i.modifiers.ctrl) {
        app.tap_tempo();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Space) && i.modifiers.ctrl) {
        if app.practice_timer_start.is_some() {
            if let Some(start) = app.practice_timer_start {
                app.settings.total_practice_secs += start.elapsed().as_secs();
                app.settings_dirty = true;
            }
            app.practice_timer_start = None;
        } else {
            app.practice_timer_start = Some(std::time::Instant::now());
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::M) && i.modifiers.ctrl) {
        app.metronome_enabled = !app.metronome_enabled;
        if app.metronome_node_id.is_none() && app.metronome_enabled {
            app.metronome_node_id = Some(app.audio_engine.add_metronome_node());
        }
        if let Some(id) = app.metronome_node_id {
            app.audio_engine.graph_set_state(
                id,
                NodeInternalState::Metronome(crate::audio::node::MetronomeNodeState {
                    bpm: app.metronome_bpm,
                    volume: app.metronome_volume,
                    count_in_beats: 0,
                    count_in_active: false,
                }),
            );
            app.audio_engine
                .graph_set_enabled(id, app.metronome_enabled);
            app.audio_engine.graph_commit_topology();
            app.audio_engine.apply_commands_to_staging();
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::R) && i.modifiers.ctrl) {
        if let Some(id) = app.recorder_node_id {
            let guard = app.audio_engine.graph.load();
            let is_recording = guard.get_node(id).map_or(false, |n| {
                matches!(&n.internal_state, crate::audio::node::NodeInternalState::Recorder(s) if s.recording)
            });
            drop(guard);
            if is_recording {
                app.audio_engine.stop_recorder(id);
            } else {
                app.audio_engine.start_recorder(id, 2);
            }
        }
    }
}
