use egui::*;

use super::ToneDockApp;
use crate::audio::node::NodeInternalState;

pub(super) fn draw_transport(app: &mut ToneDockApp, ctx: &Context) {
    TopBottomPanel::bottom("transport")
        .exact_height(42.0)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            inner_margin: Margin::symmetric(4, 2),
            stroke: Stroke::NONE,
            ..Default::default()
        })
        .show(ctx, |ui| {
            let bar_rect = ui.max_rect();
            ui.painter()
                .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(36, 38, 40));
            ui.painter().line_segment(
                [
                    pos2(bar_rect.left(), bar_rect.top()),
                    pos2(bar_rect.right(), bar_rect.top()),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                draw_metronome_section(app, ui);
                ui.separator();
                draw_looper_section(app, ui);
                ui.separator();
                draw_backing_track_section(app, ui);
                ui.separator();
                draw_drum_section(app, ui);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(id) = app.transport.looper_node_id {
                        let guard = app.audio_engine.graph.load();
                        let loop_samples = guard.looper_loop_length(id);
                        drop(guard);
                        if loop_samples > 0 {
                            let sr = app.audio_engine.sample_rate;
                            let secs = loop_samples as f64 / sr;
                            ui.label(
                                RichText::new(app.i18n.trf(
                                    "transport.loop_duration_format",
                                    &[("secs", &format!("{:.1}", secs))],
                                ))
                                .size(9.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        }
                    }
                });
            });
        });
}

fn draw_metronome_section(app: &mut ToneDockApp, ui: &mut Ui) {
    if crate::ui::controls::draw_toggle(ui, "", app.transport.metronome_enabled, 14.0) {
        app.transport.metronome_enabled = !app.transport.metronome_enabled;
        app.sync_metronome_state();
    }
    ui.label(
        RichText::new("\u{266B}")
            .size(12.0)
            .color(crate::ui::theme::ACCENT),
    );

    {
        let mut bpm = app.transport.metronome_bpm;
        ui.add_sized(
            [46.0, 16.0],
            egui::DragValue::new(&mut bpm)
                .speed(1.0)
                .range(40.0..=300.0),
        )
        .on_hover_text(app.i18n.tr("transport.bpm"));
        if (bpm - app.transport.metronome_bpm).abs() > 0.01 {
            app.transport.metronome_bpm = bpm;
            if let Some(id) = app.transport.metronome_node_id {
                let mut st = app.build_metronome_state();
                st.bpm = bpm;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Metronome(st));
            }
        }
    }

    {
        let mut vol = app.transport.metronome_volume;
        ui.add_sized([48.0, 14.0], egui::Slider::new(&mut vol, 0.0..=1.0))
            .on_hover_text(app.i18n.tr("transport.vol"));
        if (vol - app.transport.metronome_volume).abs() > 0.001 {
            app.transport.metronome_volume = vol;
            if let Some(id) = app.transport.metronome_node_id {
                let mut st = app.build_metronome_state();
                st.volume = vol;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Metronome(st));
            }
        }
    }

    {
        let mut count_in = false;
        if let Some(id) = app.transport.metronome_node_id {
            let guard = app.audio_engine.graph.load();
            if let Some(node) = guard.get_node(id) {
                if let NodeInternalState::Metronome(ms) = &node.internal_state {
                    count_in = ms.count_in_active;
                }
            }
        }
        if crate::ui::controls::draw_toggle(ui, "", count_in, 14.0) {
            if let Some(id) = app.transport.metronome_node_id {
                let mut st = app.build_metronome_state();
                st.count_in_beats = 4;
                st.count_in_active = !count_in;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Metronome(st));
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }
}

fn draw_looper_section(app: &mut ToneDockApp, ui: &mut Ui) {
    let (cur_fixed, cur_quant) = if let Some(id) = app.transport.looper_node_id {
        let guard = app.audio_engine.graph.load();
        if let Some(node) = guard.get_node(id) {
            if let NodeInternalState::Looper(st) = &node.internal_state {
                (st.fixed_length_beats, st.quantize_start)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    if crate::ui::controls::draw_toggle(ui, "", app.transport.looper_enabled, 14.0) {
        app.transport.looper_enabled = !app.transport.looper_enabled;
        if !app.transport.looper_enabled {
            app.transport.looper_recording = false;
            app.transport.looper_playing = false;
            app.transport.looper_overdubbing = false;
            if let Some(id) = app.transport.looper_node_id {
                let mut st = app.build_looper_state();
                st.enabled = false;
                st.cleared = true;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Looper(st));
            }
        } else {
            app.sync_looper_state();
        }
    }
    ui.label(
        RichText::new("LP")
            .size(10.0)
            .color(crate::ui::theme::ACCENT),
    );

    {
        let rec_fill = if app.transport.looper_recording {
            Color32::from_rgb(112, 40, 40)
        } else {
            crate::ui::theme::SURFACE_CONTAINER_HIGH
        };
        if crate::ui::controls::icon_btn_fill(
            ui,
            "\u{25CF}",
            app.i18n.tr("transport.rec"),
            rec_fill,
        )
        .clicked()
        {
            app.transport.looper_enabled = true;
            app.transport.looper_recording = !app.transport.looper_recording;
            app.transport.looper_playing = !app.transport.looper_recording;
            app.sync_looper_state();
        }
    }

    {
        let play_fill = if app.transport.looper_playing {
            Color32::from_rgb(56, 80, 62)
        } else {
            crate::ui::theme::SURFACE_CONTAINER_HIGH
        };
        if crate::ui::controls::icon_btn_fill(
            ui,
            "\u{25B6}",
            app.i18n.tr("transport.play"),
            play_fill,
        )
        .clicked()
        {
            app.transport.looper_playing = !app.transport.looper_playing;
            app.transport.looper_recording = false;
            app.sync_looper_state();
        }
    }

    {
        let dub_fill = if app.transport.looper_overdubbing {
            Color32::from_rgb(88, 72, 30)
        } else {
            crate::ui::theme::SURFACE_CONTAINER_HIGH
        };
        if crate::ui::controls::icon_btn_fill(ui, "+", app.i18n.tr("transport.overdub"), dub_fill)
            .clicked()
        {
            if app.transport.looper_playing {
                app.transport.looper_overdubbing = !app.transport.looper_overdubbing;
            }
            if let Some(id) = app.transport.looper_node_id {
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Looper(app.build_looper_state()));
            }
        }
    }

    if crate::ui::controls::icon_btn(ui, "\u{2716}", app.i18n.tr("transport.clear")).clicked() {
        app.transport.looper_recording = false;
        app.transport.looper_playing = false;
        app.transport.looper_overdubbing = false;
        if let Some(id) = app.transport.looper_node_id {
            let mut st = app.build_looper_state();
            st.enabled = false;
            st.cleared = true;
            app.audio_engine
                .graph_set_state(id, NodeInternalState::Looper(st));
        }
    }

    {
        let mut beats_str = cur_fixed.map(|b| b.to_string()).unwrap_or_default();
        if ui
            .add_sized(
                [28.0, 16.0],
                egui::TextEdit::singleline(&mut beats_str)
                    .hint_text("--")
                    .font(egui::TextStyle::Monospace),
            )
            .on_hover_text(app.i18n.tr("transport.fixed_len"))
            .changed()
        {
            let new_fixed = if beats_str.is_empty() {
                None
            } else {
                beats_str.parse::<u32>().ok()
            };
            if let Some(id) = app.transport.looper_node_id {
                let mut st = app.build_looper_state();
                st.fixed_length_beats = new_fixed;
                st.quantize_start = cur_quant;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Looper(st));
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    if crate::ui::controls::draw_toggle(ui, "", cur_quant, 12.0) {
        if let Some(id) = app.transport.looper_node_id {
            let mut st = app.build_looper_state();
            st.fixed_length_beats = cur_fixed;
            st.quantize_start = !cur_quant;
            app.audio_engine
                .graph_set_state(id, NodeInternalState::Looper(st));
            app.audio_engine.apply_commands_to_staging();
        }
    }

    if crate::ui::controls::draw_toggle(ui, "", app.transport.looper_pre_fader, 12.0) {
        app.transport.looper_pre_fader = !app.transport.looper_pre_fader;
        if let Some(id) = app.transport.looper_node_id {
            let mut st = app.build_looper_state();
            st.fixed_length_beats = cur_fixed;
            st.quantize_start = cur_quant;
            app.audio_engine
                .graph_set_state(id, NodeInternalState::Looper(st));
            app.audio_engine.apply_commands_to_staging();
        }
    }

    {
        let mut track = app.transport.looper_active_track;
        ui.add_sized(
            [22.0, 14.0],
            egui::DragValue::new(&mut track).speed(0.1).range(0..=3),
        )
        .on_hover_text("Trk");
        if track != app.transport.looper_active_track {
            app.transport.looper_active_track = track;
            if let Some(id) = app.transport.looper_node_id {
                let mut st = app.build_looper_state();
                st.fixed_length_beats = cur_fixed;
                st.quantize_start = cur_quant;
                app.audio_engine
                    .graph_set_state(id, NodeInternalState::Looper(st));
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }
}

fn draw_backing_track_section(app: &mut ToneDockApp, ui: &mut Ui) {
    ui.label(
        RichText::new("BT")
            .size(10.0)
            .color(crate::ui::theme::ACCENT),
    );

    if crate::ui::controls::icon_btn(ui, "\u{229E}", app.i18n.tr("transport.open_file")).clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                app.i18n.tr("transport.audio_files").to_owned(),
                &["wav", "mp3", "flac", "ogg", "aac", "m4a"],
            )
            .pick_file()
        {
            let id = app.audio_engine.ensure_backing_track_in_graph();
            app.transport.backing_track_node_id = Some(id);
            match app.audio_engine.load_backing_track_file(id, &path) {
                Ok(()) => {
                    app.transport.backing_track_file_name =
                        path.file_name().map(|n| n.to_string_lossy().into_owned());
                    app.transport.backing_track_duration = app.audio_engine.backing_track_duration(id);
                    app.status_message = app.i18n.trf(
                        "status.loaded_backing_track",
                        &[(
                            "name",
                            &app.transport.backing_track_file_name.as_deref().unwrap_or("?"),
                        )],
                    );
                }
                Err(e) => {
                    app.status_message = app
                        .i18n
                        .trf("status.backing_track_error", &[("error", &e.to_string())]);
                    log::error!("Failed to load backing track: {}", e);
                }
            }
        }
    }

    if let Some(name) = &app.transport.backing_track_file_name {
        let display_name = if name.len() > 14 {
            format!("{}...", &name[..11])
        } else {
            name.clone()
        };
        ui.label(
            RichText::new(display_name)
                .size(9.0)
                .color(crate::ui::theme::TEXT_SECONDARY),
        );
    }

    {
        let play_fill = if app.transport.backing_track_playing {
            Color32::from_rgb(56, 80, 62)
        } else {
            crate::ui::theme::SURFACE_CONTAINER_HIGH
        };
        if crate::ui::controls::icon_btn_fill(
            ui,
            "\u{25B6}",
            app.i18n.tr("transport.play"),
            play_fill,
        )
        .clicked()
        {
            if app.transport.backing_track_node_id.is_some() {
                app.transport.backing_track_playing = !app.transport.backing_track_playing;
                app.sync_backing_track_state();
            }
        }
    }

    if crate::ui::controls::icon_btn(ui, "\u{25A0}", app.i18n.tr("transport.stop")).clicked() {
        if let Some(id) = app.transport.backing_track_node_id {
            app.transport.backing_track_playing = false;
            app.audio_engine.backing_track_seek(id, 0.0);
            app.sync_backing_track_state();
        }
    }

    {
        let mut vol = app.transport.backing_track_volume;
        ui.add_sized([40.0, 14.0], egui::Slider::new(&mut vol, 0.0..=1.0))
            .on_hover_text(app.i18n.tr("transport.vol"));
        if (vol - app.transport.backing_track_volume).abs() > 0.001 {
            app.transport.backing_track_volume = vol;
            if let Some(id) = app.transport.backing_track_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::BackingTrack(app.build_backing_track_state()),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    {
        let mut speed = app.transport.backing_track_speed;
        ui.add_sized(
            [34.0, 14.0],
            egui::DragValue::new(&mut speed)
                .speed(0.05)
                .range(0.25..=2.0),
        )
        .on_hover_text(app.i18n.tr("transport.speed"));
        if (speed - app.transport.backing_track_speed).abs() > 0.001 {
            app.transport.backing_track_speed = speed;
            if let Some(id) = app.transport.backing_track_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::BackingTrack(app.build_backing_track_state()),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    if crate::ui::controls::draw_toggle(ui, "", app.transport.backing_track_looping, 14.0) {
        app.transport.backing_track_looping = !app.transport.backing_track_looping;
        if app.transport.backing_track_node_id.is_some() {
            app.sync_backing_track_state();
        }
    }

    {
        let mut pitch = app.transport.backing_track_pitch_semitones;
        ui.add_sized(
            [34.0, 14.0],
            egui::DragValue::new(&mut pitch)
                .speed(0.5)
                .range(-12.0..=12.0)
                .suffix("st"),
        )
        .on_hover_text(app.i18n.tr("transport.pitch"));
        if (pitch - app.transport.backing_track_pitch_semitones).abs() > 0.01 {
            app.transport.backing_track_pitch_semitones = pitch;
            if let Some(id) = app.transport.backing_track_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::BackingTrack(app.build_backing_track_state()),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    {
        let mut pre_roll = app.transport.backing_track_pre_roll_secs;
        ui.add_sized(
            [34.0, 14.0],
            egui::DragValue::new(&mut pre_roll)
                .speed(0.5)
                .range(0.0..=10.0)
                .suffix("s"),
        )
        .on_hover_text(app.i18n.tr("transport.pre_roll"));
        if (pre_roll - app.transport.backing_track_pre_roll_secs).abs() > 0.01 {
            app.transport.backing_track_pre_roll_secs = pre_roll;
            if let Some(id) = app.transport.backing_track_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::BackingTrack(app.build_backing_track_state()),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    if let Some(id) = app.transport.backing_track_node_id {
        let pos = app.audio_engine.backing_track_position(id);
        let dur = app.transport.backing_track_duration;
        if dur > 0.0 {
            let pos_mins = (pos / 60.0) as u32;
            let pos_secs = (pos % 60.0) as u32;
            let dur_mins = (dur / 60.0) as u32;
            let dur_secs = (dur % 60.0) as u32;
            ui.label(
                RichText::new(app.i18n.trf(
                    "transport.time_format",
                    &[
                        ("pos_mins", &pos_mins.to_string()),
                        ("pos_secs", &format!("{:02}", pos_secs)),
                        ("dur_mins", &dur_mins.to_string()),
                        ("dur_secs", &format!("{:02}", dur_secs)),
                    ],
                ))
                .size(9.0)
                .color(crate::ui::theme::TEXT_SECONDARY),
            );
        }

        let (cur_ab_start, cur_ab_end) = {
            let guard = app.audio_engine.graph.load();
            if let Some(node) = guard.get_node(id) {
                if let NodeInternalState::BackingTrack(st) = &node.internal_state {
                    (st.loop_start, st.loop_end)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        };

        let ab_label = if cur_ab_start.is_some() && cur_ab_end.is_some() {
            "A\u{2011}B \u{2716}"
        } else if cur_ab_start.is_some() {
            "B\u{2192}"
        } else {
            "A\u{2192}"
        };
        if ui
            .add_sized([32.0, 18.0], Button::new(RichText::new(ab_label).size(9.0)))
            .on_hover_text(app.i18n.tr("transport.ab_set_a"))
            .clicked()
        {
            let (new_start, new_end) = if cur_ab_start.is_some() && cur_ab_end.is_some() {
                (None, None)
            } else if cur_ab_start.is_some() {
                (cur_ab_start, Some(pos))
            } else {
                (Some(pos), None)
            };
            let mut st = app.build_backing_track_state();
            st.loop_start = new_start;
            st.loop_end = new_end;
            app.audio_engine
                .graph_set_state(id, NodeInternalState::BackingTrack(st));
            app.audio_engine.apply_commands_to_staging();
        }

        let cur_markers = {
            let guard = app.audio_engine.graph.load();
            if let Some(node) = guard.get_node(id) {
                if let NodeInternalState::BackingTrack(st) = &node.internal_state {
                    st.section_markers.clone()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        if crate::ui::controls::icon_btn(ui, "+M", app.i18n.tr("transport.add_marker")).clicked() {
            let mut markers = cur_markers.clone();
            markers.push(pos);
            markers.sort_by(|a, b| a.partial_cmp(b).unwrap());
            app.transport.backing_track_section_markers = markers.clone();
            let mut st = app.build_backing_track_state();
            st.loop_start = cur_ab_start;
            st.loop_end = cur_ab_end;
            st.section_markers = markers;
            app.audio_engine
                .graph_set_state(id, NodeInternalState::BackingTrack(st));
            app.audio_engine.apply_commands_to_staging();
        }

        for marker in cur_markers.iter() {
            let mins = (*marker / 60.0) as u32;
            let secs = (*marker % 60.0) as u32;
            let label = format!("{}:{:02}", mins, secs);
            if ui.button(RichText::new(label).size(8.0)).clicked() {
                app.audio_engine.backing_track_seek(id, *marker);
            }
        }
    }
}

fn draw_drum_section(app: &mut ToneDockApp, ui: &mut Ui) {
    let (drum_playing, drum_bpm, drum_vol, drum_pattern) = if let Some(id) =
        app.transport.drum_machine_node_id
    {
        let guard = app.audio_engine.graph.load();
        if let Some(node) = guard.get_node(id) {
            if let crate::audio::node::NodeInternalState::DrumMachine(st) = &node.internal_state {
                (st.playing, st.bpm, st.volume, st.pattern)
            } else {
                (false, 120.0, 0.8, 0)
            }
        } else {
            (false, 120.0, 0.8, 0)
        }
    } else {
        (false, 120.0, 0.8, 0)
    };

    if crate::ui::controls::draw_toggle(ui, "", drum_playing, 14.0) {
        if app.transport.drum_machine_node_id.is_none() {
            app.transport.drum_machine_node_id = Some(app.audio_engine.add_drum_machine_node());
        }
        if let Some(id) = app.transport.drum_machine_node_id {
            app.audio_engine.graph_set_state(
                id,
                NodeInternalState::DrumMachine(crate::audio::node::DrumMachineNodeState {
                    bpm: drum_bpm,
                    volume: drum_vol,
                    playing: !drum_playing,
                    pattern: drum_pattern,
                    current_step: 0,
                }),
            );
            app.audio_engine.apply_commands_to_staging();
        }
    }
    ui.label(
        RichText::new("DR")
            .size(10.0)
            .color(crate::ui::theme::ACCENT),
    );

    {
        let mut bpm = drum_bpm;
        ui.add_sized(
            [40.0, 14.0],
            egui::DragValue::new(&mut bpm)
                .speed(1.0)
                .range(40.0..=300.0),
        )
        .on_hover_text(app.i18n.tr("transport.drum_bpm"));
        if (bpm - drum_bpm).abs() > 0.01 {
            if let Some(id) = app.transport.drum_machine_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::DrumMachine(crate::audio::node::DrumMachineNodeState {
                        bpm,
                        volume: drum_vol,
                        playing: drum_playing,
                        pattern: drum_pattern,
                        current_step: 0,
                    }),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    {
        let mut vol = drum_vol;
        ui.add_sized([36.0, 14.0], egui::Slider::new(&mut vol, 0.0..=1.0))
            .on_hover_text(app.i18n.tr("transport.drum_vol"));
        if (vol - drum_vol).abs() > 0.001 {
            if let Some(id) = app.transport.drum_machine_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::DrumMachine(crate::audio::node::DrumMachineNodeState {
                        bpm: drum_bpm,
                        volume: vol,
                        playing: drum_playing,
                        pattern: drum_pattern,
                        current_step: 0,
                    }),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }

    {
        let mut pat = drum_pattern;
        ui.add_sized(
            [22.0, 14.0],
            egui::DragValue::new(&mut pat).speed(0.1).range(0..=4),
        )
        .on_hover_text(app.i18n.tr("transport.drum_pattern"));
        if pat != drum_pattern {
            if let Some(id) = app.transport.drum_machine_node_id {
                app.audio_engine.graph_set_state(
                    id,
                    NodeInternalState::DrumMachine(crate::audio::node::DrumMachineNodeState {
                        bpm: drum_bpm,
                        volume: drum_vol,
                        playing: drum_playing,
                        pattern: pat,
                        current_step: 0,
                    }),
                );
                app.audio_engine.apply_commands_to_staging();
            }
        }
    }
}
