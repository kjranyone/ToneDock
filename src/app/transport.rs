use egui::*;

use super::ToneDockApp;
use crate::audio::node::{LooperNodeState, NodeInternalState, MetronomeNodeState};

use super::toolbar::ui_section_frame;

pub(super) fn draw_transport(app: &mut ToneDockApp, ctx: &Context) {
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
                            {
                                let mut met = app.audio_engine.metronome.lock();
                                met.enabled = app.metronome_enabled;
                                met.set_bpm(app.metronome_bpm);
                                met.volume = app.metronome_volume;
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
                            app.audio_engine.metronome.lock().set_bpm(bpm);
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
                            app.audio_engine.metronome.lock().volume = vol;
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
                            {
                                let mut lpr = app.audio_engine.looper.lock();
                                lpr.enabled = app.looper_enabled;
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
                                app.audio_engine.looper.lock().clear();
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
                            {
                                let mut lpr = app.audio_engine.looper.lock();
                                lpr.enabled = true;
                                lpr.toggle_record();
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
                            {
                                let mut lpr = app.audio_engine.looper.lock();
                                lpr.enabled = true;
                                lpr.toggle_play();
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
                            app.audio_engine.looper.lock().toggle_overdub();
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
                            app.audio_engine.looper.lock().clear();
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
