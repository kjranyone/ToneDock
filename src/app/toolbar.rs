use eframe::App;
use egui::*;
use std::sync::atomic::Ordering;

use super::{ToneDockApp, ViewMode};

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
        .exact_height(36.0)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            inner_margin: Margin::symmetric(4, 2),
            stroke: Stroke::NONE,
            ..Default::default()
        })
        .show(ctx, |ui| {
            let bar_rect = ui.max_rect();
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(34, 36, 39));
            ui.painter().line_segment(
                [
                    pos2(bar_rect.left(), bar_rect.bottom() - 1.0),
                    pos2(bar_rect.right(), bar_rect.bottom() - 1.0),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                ui.label(
                    RichText::new(app.i18n.tr("app.title"))
                        .size(13.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );

                ui.separator();

                if crate::ui::controls::icon_btn(ui, "\u{25BC}", app.i18n.tr("toolbar.save_preset")).clicked() {
                    app.save_preset();
                }
                if crate::ui::controls::icon_btn(ui, "\u{25B2}", app.i18n.tr("toolbar.load_preset")).clicked() {
                    app.load_preset();
                }
                if crate::ui::controls::icon_btn(ui, "\u{21E9}", app.i18n.tr("toolbar.import_session")).clicked() {
                    app.import_session();
                }

                ui.separator();

                if crate::ui::controls::icon_btn(ui, "\u{229E}", app.i18n.tr("toolbar.save_workspace")).clicked() {
                    app.save_workspace();
                }
                if crate::ui::controls::icon_btn(ui, "\u{229F}", app.i18n.tr("toolbar.load_workspace")).clicked() {
                    app.load_workspace();
                }

                ui.separator();

                if crate::ui::controls::icon_btn(ui, "S\u{2191}", app.i18n.tr("toolbar.ab_snap_a")).clicked() {
                    app.snapshot_to_ab('a');
                }
                if crate::ui::controls::icon_btn(ui, "S\u{2193}", app.i18n.tr("toolbar.ab_snap_b")).clicked() {
                    app.snapshot_to_ab('b');
                }
                if crate::ui::controls::icon_btn(ui, "\u{2190}A", app.i18n.tr("toolbar.ab_restore_a")).clicked() {
                    app.restore_ab('a');
                }
                if crate::ui::controls::icon_btn(ui, "\u{2190}B", app.i18n.tr("toolbar.ab_restore_b")).clicked() {
                    app.restore_ab('b');
                }

                let suggestions = app.suggest_plugin_chain();
                if !suggestions.is_empty() {
                    ui.menu_button("AI", |ui| {
                        for suggestion in &suggestions {
                            ui.label(RichText::new(suggestion).size(9.0));
                        }
                    });
                }

                let template_labels: Vec<(String, String)> =
                    crate::app::ToneDockApp::practice_template_names()
                        .iter()
                        .map(|(key, label_key)| {
                            (key.to_string(), app.i18n.tr(label_key).to_owned())
                        })
                        .collect();
                ui.menu_button("\u{266B}", |ui| {
                    for (key, label) in &template_labels {
                        let key = key.clone();
                        if ui.button(label.as_str()).clicked() {
                            app.apply_practice_template(&key);
                            ui.close_menu();
                        }
                    }
                });

                if crate::ui::controls::icon_btn(ui, "\u{2699}", app.i18n.tr("toolbar.settings")).clicked() {
                    app.open_preferences();
                }

                ui.separator();

                {
                    let running = app.audio_engine.is_running();
                    let (icon, fill) = if running {
                        ("\u{25A0}", Color32::from_rgb(88, 42, 42))
                    } else {
                        ("\u{25B6}", Color32::from_rgb(48, 78, 56))
                    };
                    if ui
                        .add_sized([28.0, 24.0], Button::new(RichText::new(icon).size(14.0)).fill(fill))
                        .on_hover_text(if running {
                            app.i18n.tr("toolbar.stop_audio")
                        } else {
                            app.i18n.tr("toolbar.start_audio")
                        })
                        .clicked()
                    {
                        if running {
                            app.audio_engine.stop();
                        } else {
                            app.start_audio();
                        }
                    }
                }

                {
                    let mut vol = app.master_volume;
                    ui.add_sized(
                        [60.0, 14.0],
                        egui::Slider::new(&mut vol, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .on_hover_text(app.i18n.tr("toolbar.master"));
                    if (vol - app.master_volume).abs() > 0.001 {
                        app.master_volume = vol;
                        app.audio_engine.master_volume.store(vol.to_bits(), Ordering::Relaxed);
                    }
                }

                {
                    let mut gain = app.input_gain;
                    ui.add_sized(
                        [36.0, 16.0],
                        egui::DragValue::new(&mut gain).speed(0.01).range(0.0..=4.0),
                    )
                    .on_hover_text(app.i18n.tr("toolbar.gain"));
                    if (gain - app.input_gain).abs() > 0.001 {
                        app.input_gain = gain;
                        app.audio_engine.input_gain.store(gain.to_bits(), Ordering::Relaxed);
                    }
                }

                if crate::ui::controls::icon_btn(ui, "TAP", app.i18n.tr("toolbar.tap_tempo")).clicked() {
                    app.tap_tempo();
                }

                {
                    let mut bpm_goal = app.settings.bpm_goal.unwrap_or(0.0);
                    ui.add_sized(
                        [44.0, 16.0],
                        egui::DragValue::new(&mut bpm_goal)
                            .speed(1.0)
                            .range(0.0..=300.0)
                            .custom_formatter(|v, _| {
                                if v < 1.0 {
                                    "—".into()
                                } else {
                                    format!("{:.0} BPM", v)
                                }
                            })
                            .custom_parser(|s| {
                                if s == "—" {
                                    Some(0.0)
                                } else {
                                    s.trim_end_matches(" BPM").parse().ok()
                                }
                            }),
                    )
                    .on_hover_text(app.i18n.tr("toolbar.bpm_goal"));
                    let new_goal = if bpm_goal < 1.0 { None } else { Some(bpm_goal) };
                    if new_goal != app.settings.bpm_goal {
                        app.settings.bpm_goal = new_goal;
                        app.settings_dirty = true;
                    }
                }

                if let Some(goal) = app.settings.bpm_goal {
                    if goal > 0.0 && (app.transport.metronome_bpm - goal).abs() < 0.5 {
                        ui.label(
                            RichText::new(app.i18n.tr("toolbar.goal_reached"))
                                .size(9.0)
                                .color(crate::ui::theme::ACCENT_WARM)
                                .strong(),
                        );
                    }
                }

                {
                    let timer_icon = if app.practice_timer_start.is_some() {
                        "\u{23F1}"
                    } else {
                        "\u{25F7}"
                    };
                    if crate::ui::controls::icon_btn(ui, timer_icon, if app.practice_timer_start.is_some() {
                        app.i18n.tr("toolbar.timer_stop")
                    } else {
                        app.i18n.tr("toolbar.timer_start")
                    })
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
                }

                ui.separator();

                if app.transport.recorder_node_id.is_none() {
                    if crate::ui::controls::icon_btn(ui, "\u{25CF}+", app.i18n.tr("toolbar.add_recorder")).clicked() {
                        app.transport.recorder_node_id = Some(app.audio_engine.add_recorder_node());
                    }
                } else {
                    let is_recording = if let Some(id) = app.transport.recorder_node_id {
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
                    let rec_icon = if is_recording { "\u{25A0}" } else { "\u{25CF}" };
                    if ui
                        .add_sized([28.0, 24.0], Button::new(RichText::new(rec_icon).size(14.0)).fill(rec_fill))
                        .on_hover_text(if is_recording {
                            app.i18n.tr("toolbar.stop_rec")
                        } else {
                            app.i18n.tr("toolbar.start_rec")
                        })
                        .clicked()
                    {
                        if let Some(id) = app.transport.recorder_node_id {
                            if is_recording {
                                app.audio_engine.stop_recorder(id);
                                app.status_message = app.i18n.tr("status.recording_stopped").into();
                            } else {
                                app.audio_engine.start_recorder(id, 2);
                                app.status_message = app.i18n.tr("status.recording_started").into();
                            }
                        }
                    }
                    if crate::ui::controls::icon_btn(ui, "\u{21E4}", app.i18n.tr("toolbar.export_rec")).clicked() {
                        if let Some(id) = app.transport.recorder_node_id {
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

                ui.separator();

                {
                    let view_icon = match app.view_mode {
                        ViewMode::Rack => "\u{25E8}",
                        ViewMode::NodeEditor => "\u{25CB}",
                    };
                    if crate::ui::controls::icon_btn(
                        ui,
                        view_icon,
                        match app.view_mode {
                            ViewMode::Rack => app.i18n.tr("toolbar.node_view"),
                            ViewMode::NodeEditor => app.i18n.tr("toolbar.rack_view"),
                        },
                    )
                    .clicked()
                    {
                        app.view_mode = match app.view_mode {
                            ViewMode::Rack => {
                                if app.rack.inline_gui {
                                    app.close_all_rack_editors();
                                }
                                app.node_editor.set_selection(app.rack.selected_node);
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
                }
                if crate::ui::controls::icon_btn(ui, "i", app.i18n.tr("toolbar.about")).clicked() {
                    app.show_about = true;
                }
                if crate::ui::controls::icon_btn(ui, "\u{2922}", app.i18n.tr("toolbar.fullscreen")).clicked() {
                    app.fullscreen = !app.fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(app.fullscreen));
                }

                ui.separator();

                let can_undo = app.undo_manager.can_undo();
                let can_redo = app.undo_manager.can_redo();
                ui.add_enabled_ui(can_undo, |ui| {
                    if crate::ui::controls::icon_btn(ui, "\u{21A9}", app.i18n.tr("undo.edit")).clicked() {
                        app.perform_undo();
                    }
                });
                ui.add_enabled_ui(can_redo, |ui| {
                    if crate::ui::controls::icon_btn(ui, "\u{21AA}", app.i18n.tr("undo.edit")).clicked() {
                        app.perform_redo();
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let dropouts = app.audio_engine.dropout_count.load(Ordering::Relaxed);
                    if dropouts > app.last_dropout_count {
                        app.status_message = app.i18n.trf(
                            "status.dropout_detected",
                            &[("count", &dropouts.to_string())],
                        );
                        app.last_dropout_count = dropouts;
                    }

                    if app.scanning_in_progress {
                        ui.spinner();
                    }

                    ui.label(
                        RichText::new(&app.status_message)
                            .size(9.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );

                    let current_elapsed = app.practice_timer_start.map_or(0, |s| s.elapsed().as_secs());

                    if current_elapsed > 0 {
                        let secs = current_elapsed;
                        let mins = secs / 60;
                        let secs = secs % 60;
                        ui.label(
                            RichText::new(app.i18n.trf("toolbar.timer_format", &[
                                ("mins", &mins.to_string()),
                                ("secs", &format!("{:02}", secs)),
                            ]))
                                .size(9.0)
                                .color(crate::ui::theme::ACCENT_WARM),
                        );
                    }

                    {
                        let total = app.settings.total_practice_secs + current_elapsed;
                        if total > 0 {
                            let hours = total / 3600;
                            let mins = (total % 3600) / 60;
                            ui.label(
                                RichText::new(app.i18n.trf("toolbar.total_practice", &[
                                    ("hours", &hours.to_string()),
                                    ("mins", &format!("{:02}", mins)),
                                ]))
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        }
                    }

                    let cpu = f32::from_bits(app.audio_engine.cpu_usage.load(Ordering::Relaxed));
                    let cpu_color = if cpu > 80.0 {
                        crate::ui::theme::METER_RED
                    } else if cpu > 50.0 {
                        crate::ui::theme::METER_YELLOW
                    } else {
                        crate::ui::theme::METER_GREEN
                    };
                    ui.label(
                        RichText::new(app.i18n.trf("toolbar.cpu_format", &[
                            ("cpu", &format!("{:.0}", cpu)),
                        ]))
                            .size(9.0)
                            .color(cpu_color),
                    );

                    let latency_ms = app.audio_engine.buffer_size as f64
                        / app.audio_engine.sample_rate
                        * 1000.0;
                    ui.label(
                        RichText::new(app.i18n.trf("toolbar.latency_format", &[
                            ("ms", &format!("{:.1}", latency_ms)),
                        ]))
                            .size(9.0)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
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
        app.transport.metronome_enabled = !app.transport.metronome_enabled;
        app.sync_metronome_state();
    }
    if ctx.input(|i| i.key_pressed(egui::Key::R) && i.modifiers.ctrl) {
        if let Some(id) = app.transport.recorder_node_id {
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
