use eframe::App;
use egui::*;

use super::{ToneDockApp, ViewMode};

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
                        if ui.button(app.i18n.tr("toolbar.settings")).clicked() {
                            app.open_preferences();
                        }
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
