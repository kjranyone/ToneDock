use egui::*;
use std::sync::atomic::Ordering;

use super::ToneDockApp;
use crate::audio::node::{NodeId, NodeType};
use crate::vst_host::editor::PluginEditor;

impl ToneDockApp {
    pub(crate) fn open_rack_editor(&mut self, node_id: NodeId) {
        if self.rack.inline_gui {
            if self.rack.inline_editor_node != Some(node_id) {
                self.close_all_rack_editors();
            }
            self.rack.inline_editor_node = Some(node_id);
            return;
        }

        let (edit_controller, plugin_name) = {
            let guard = self.audio_engine.graph.load();
            let Some(node) = guard.get_node(node_id) else {
                return;
            };
            let plugin_name = match &node.node_type {
                NodeType::VstPlugin { plugin_name, .. } => plugin_name.clone(),
                _ => return,
            };
            let edit_controller = node
                .plugin_instance
                .lock()
                .as_ref()
                .and_then(|plugin| plugin.edit_controller().cloned());
            (edit_controller, plugin_name)
        };

        if let Some(edit_controller) = edit_controller {
            let editor = self
                .rack
                .plugin_editors
                .entry(node_id)
                .or_insert_with(PluginEditor::new);
            match editor.open_separate_window(
                &edit_controller,
                &plugin_name,
                self.main_hwnd.map(|h| h.as_ptr()),
            ) {
                Ok(()) => {
                    self.status_message = self
                        .i18n
                        .trf("status.opened_editor", &[("name", &plugin_name)]);
                }
                Err(err) => {
                    log::error!("Failed to open editor for '{}': {}", plugin_name, err);
                    self.status_message = self
                        .i18n
                        .trf("status.editor_error", &[("error", &err.to_string())]);
                }
            }
        }
    }

    pub(crate) fn ensure_inline_rack_editor(&mut self, ui: &Ui, node_id: NodeId, rect: Rect) {
        if !self.rack.inline_gui {
            return;
        }

        let Some(main_hwnd) = self.main_hwnd else {
            return;
        };

        let (edit_controller, plugin_name) = {
            let guard = self.audio_engine.graph.load();
            let Some(node) = guard.get_node(node_id) else {
                return;
            };
            let plugin_name = match &node.node_type {
                NodeType::VstPlugin { plugin_name, .. } => plugin_name.clone(),
                _ => return,
            };
            let edit_controller = node
                .plugin_instance
                .lock()
                .as_ref()
                .and_then(|plugin| plugin.edit_controller().cloned());
            (edit_controller, plugin_name)
        };

        let Some(edit_controller) = edit_controller else {
            return;
        };

        let pixels_per_point = ui.ctx().pixels_per_point();
        let bounds = (
            (rect.left() * pixels_per_point).round() as i32,
            (rect.top() * pixels_per_point).round() as i32,
            (rect.width() * pixels_per_point).round().max(1.0) as i32,
            (rect.height() * pixels_per_point).round().max(1.0) as i32,
        );

        let editor = self
            .rack
            .plugin_editors
            .entry(node_id)
            .or_insert_with(PluginEditor::new);

        if editor.is_open() {
            editor.set_embedded_bounds(bounds);
            return;
        }

        if let Err(embed_err) =
            editor.open_embedded_window(&edit_controller, &plugin_name, main_hwnd.as_ptr(), bounds)
        {
            log::error!(
                "Failed to open inline editor for '{}': {}",
                plugin_name,
                embed_err
            );
            self.rack.inline_editor_node = None;
            match editor.open_separate_window(
                &edit_controller,
                &plugin_name,
                self.main_hwnd.map(|h| h.as_ptr()),
            ) {
                Ok(()) => {
                    self.status_message = self
                        .i18n
                        .trf("status.inline_gui_fallback", &[("name", &plugin_name)]);
                }
                Err(sep_err) => {
                    log::error!(
                        "Fallback separate window also failed for '{}': {}",
                        plugin_name,
                        sep_err
                    );
                    self.status_message = self
                        .i18n
                        .trf("status.editor_error", &[("error", &sep_err.to_string())]);
                }
            }
        }
    }

    pub(crate) fn show_rack_view(&mut self, ui: &mut Ui) {
        let screen_size = ui.ctx().screen_rect().size();
        let side_width = 280.0;
        let full_height = ui.available_height();
        crate::ui::theme::paint_panel_texture(ui.painter(), ui.max_rect());
        crate::ui::theme::paint_rack_bay(ui.painter(), ui.max_rect().shrink2(vec2(12.0, 12.0)));

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(screen_size.x - side_width - 36.0);

                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 12, 16, 205))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                        ui.set_min_height(full_height - 36.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(self.i18n.tr("rack.digital_rack"))
                                    .size(12.0)
                                    .strong()
                                    .color(crate::ui::theme::ACCENT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(self.i18n.trf(
                                        "rack.plugins_available",
                                        &[("count", &self.available_plugins.len().to_string())],
                                    ))
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                                );
                            });
                        });
                        ui.add_space(10.0);

                        let available = self.available_plugins.clone();
                        let rack_slots = self.build_rack_slots();
                        let inline_node = self
                            .rack
                            .inline_editor_node
                            .filter(|node_id| self.rack.order.contains(node_id));
                        let (commands, inline_rects) = self.rack_view.show(
                            ui,
                            &rack_slots,
                            &available,
                            inline_node,
                            &self.i18n,
                        );

                        for (node_id, rect) in &inline_rects {
                            self.ensure_inline_rack_editor(ui, *node_id, *rect);
                        }

                        for cmd in commands {
                            match cmd {
                                crate::ui::rack_view::RackCommand::Select(node_id) => {
                                    self.select_rack_plugin_node(Some(node_id));
                                }
                                crate::ui::rack_view::RackCommand::Add(plugin_idx) => {
                                    if let Some(info) = available.get(plugin_idx).cloned() {
                                        match self.add_rack_plugin_to_graph(&info) {
                                            Ok(node_id) => {
                                                self.select_rack_plugin_node(Some(node_id));
                                                self.status_message = self
                                                    .i18n
                                                    .trf("status.loaded", &[("name", &info.name)]);
                                            }
                                            Err(err) => {
                                                log::error!(
                                                    "Load error for {}: {}",
                                                    info.name,
                                                    err
                                                );
                                                self.status_message = self.i18n.trf(
                                                    "status.load_error",
                                                    &[("error", &err.to_string())],
                                                );
                                            }
                                        }
                                    }
                                }
                                crate::ui::rack_view::RackCommand::Remove(node_id) => {
                                    self.remove_rack_plugin_from_graph(node_id);
                                    if self.rack.selected_node == Some(node_id) {
                                        self.select_rack_plugin_node(None);
                                    }
                                }
                                crate::ui::rack_view::RackCommand::Reorder(
                                    node_id,
                                    target_index,
                                ) => {
                                    self.reorder_rack_plugin(node_id, target_index);
                                }
                                crate::ui::rack_view::RackCommand::ToggleBypass(node_id) => {
                                    let state = {
                                        let guard = self.audio_engine.graph.load();
                                        guard
                                            .get_node(node_id)
                                            .map(|node| (node.enabled, !node.bypassed))
                                    };
                                    if let Some((enabled, bypassed)) = state {
                                        self.sync_rack_plugin_state(node_id, enabled, bypassed);
                                    }
                                }
                                crate::ui::rack_view::RackCommand::ToggleEnable(node_id) => {
                                    let state = {
                                        let guard = self.audio_engine.graph.load();
                                        guard
                                            .get_node(node_id)
                                            .map(|node| (!node.enabled, node.bypassed))
                                    };
                                    if let Some((enabled, bypassed)) = state {
                                        self.sync_rack_plugin_state(node_id, enabled, bypassed);
                                    }
                                }
                                crate::ui::rack_view::RackCommand::OpenEditor(node_id) => {
                                    self.open_rack_editor(node_id);
                                }
                                crate::ui::rack_view::RackCommand::CloseEditor(node_id) => {
                                    self.close_rack_editor(node_id);
                                    self.status_message =
                                        self.i18n.tr("status.closed_editor").into();
                                }
                            }
                        }
                    });
            });

            ui.vertical(|ui| {
                ui.set_max_width(side_width);

                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(20, 20, 24, 230))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_min_height(full_height - 28.0);
                        ui.label(
                            RichText::new(self.i18n.tr("rack.rack_control"))
                                .size(11.0)
                                .strong()
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(8.0);

                        let (out_l, out_r) = (
                            f32::from_bits(
                                self.audio_engine.output_level_l.load(Ordering::Relaxed),
                            ),
                            f32::from_bits(
                                self.audio_engine.output_level_r.load(Ordering::Relaxed),
                            ),
                        );
                        crate::ui::meters::draw_stereo_meter(
                            ui,
                            self.i18n.tr("rack.master_out"),
                            out_l,
                            out_r,
                            side_width - 28.0,
                            68.0,
                        );

                        ui.add_space(6.0);

                        let in_l =
                            f32::from_bits(self.audio_engine.input_level_l.load(Ordering::Relaxed));
                        crate::ui::meters::draw_mono_meter(
                            ui,
                            self.i18n.tr("rack.mono_input"),
                            in_l,
                            side_width - 28.0,
                            68.0,
                        );

                        ui.add_space(10.0);

                        let selected_node = self.rack.selected_node.or_else(|| {
                            (self.audio_engine.chain_node_ids.len() == 1)
                                .then(|| self.audio_engine.chain_node_ids[0])
                        });
                        if let Some(node_id) = selected_node {
                            self.draw_vst_parameter_panel(ui, node_id);
                        } else {
                            Frame::group(ui.style())
                                .fill(crate::ui::theme::BG_PANEL)
                                .inner_margin(16.0)
                                .corner_radius(CornerRadius::same(14))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new(self.i18n.tr("rack.no_module_selected"))
                                                .size(13.0)
                                                .color(crate::ui::theme::TEXT_SECONDARY),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(self.i18n.tr("rack.select_hint"))
                                                .size(10.0)
                                                .color(crate::ui::theme::TEXT_HINT),
                                        );
                                    });
                                });
                        }
                    });
            });
        });
    }

    pub(crate) fn show_node_editor(&mut self, ui: &mut Ui) {
        let (snaps, conns) = {
            let guard = self.audio_engine.graph.load();
            let snaps: Vec<crate::ui::node_editor::NodeSnap> = guard
                .nodes()
                .iter()
                .map(|(&id, node)| crate::ui::node_editor::NodeSnap {
                    id,
                    node_type: node.node_type.clone(),
                    enabled: node.enabled,
                    bypassed: node.bypassed,
                    pos: node.position,
                    inputs: node.input_ports.clone(),
                    outputs: node.output_ports.clone(),
                    state: node.internal_state.clone(),
                })
                .collect();
            let conns: Vec<crate::audio::node::Connection> = guard.connections().to_vec();
            (snaps, conns)
        };

        let side_width = 280.0;
        let screen_size = ui.ctx().screen_rect().size();
        let full_w = screen_size.x - side_width - 30.0;
        let full_height = ui.available_height();
        crate::ui::theme::paint_panel_texture(ui.painter(), ui.max_rect());

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(full_w);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(10, 10, 14, 220))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_min_height(full_height - 16.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(self.i18n.tr("rack.signal_flow"))
                                    .size(12.0)
                                    .strong()
                                    .color(crate::ui::theme::ACCENT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(self.i18n.tr("rack.hardware_canvas"))
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_HINT),
                                );
                            });
                        });
                        ui.add_space(8.0);
                        let cmds = self.node_editor.show(
                            ui,
                            &snaps,
                            &conns,
                            &self.available_plugins,
                            &self.i18n,
                        );
                        self.process_editor_commands(cmds);
                    });
            });

            ui.vertical(|ui| {
                ui.set_max_width(side_width);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 18, 24, 235))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_min_height(full_height - 28.0);
                        ui.label(
                            RichText::new(self.i18n.tr("rack.node_inspector"))
                                .size(11.0)
                                .strong()
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(8.0);

                        let (out_l, out_r) = (
                            f32::from_bits(
                                self.audio_engine.output_level_l.load(Ordering::Relaxed),
                            ),
                            f32::from_bits(
                                self.audio_engine.output_level_r.load(Ordering::Relaxed),
                            ),
                        );
                        crate::ui::meters::draw_stereo_meter(
                            ui,
                            self.i18n.tr("rack.output"),
                            out_l,
                            out_r,
                            side_width - 28.0,
                            48.0,
                        );
                        ui.add_space(4.0);
                        let in_l =
                            f32::from_bits(self.audio_engine.input_level_l.load(Ordering::Relaxed));
                        crate::ui::meters::draw_mono_meter(
                            ui,
                            self.i18n.tr("rack.mono_input"),
                            in_l,
                            side_width - 28.0,
                            48.0,
                        );

                        ui.add_space(10.0);

                        if let Some(sel_id) = self.node_editor.selected_node() {
                            self.draw_vst_parameter_panel(ui, sel_id);
                        } else {
                            ui.label(
                                RichText::new(self.i18n.tr("rack.select_node"))
                                    .size(11.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        }
                    });
            });
        });
    }

    pub(crate) fn draw_vst_parameter_panel(&mut self, ui: &mut Ui, node_id: NodeId) {
        let node_name = {
            let guard = self.audio_engine.graph.load();
            guard
                .get_node(node_id)
                .map(|n| match &n.node_type {
                    NodeType::VstPlugin { plugin_name, .. } => plugin_name.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default()
        };

        if node_name.is_empty() {
            return;
        }

        let has_plugin = {
            let guard = self.audio_engine.graph.load();
            guard
                .get_node(node_id)
                .map(|n| n.plugin_instance.lock().is_some())
                .unwrap_or(false)
        };

        if !has_plugin {
            Frame::group(ui.style())
                .fill(crate::ui::theme::BG_PANEL)
                .inner_margin(12.0)
                .corner_radius(CornerRadius::same(14))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(&node_name)
                                .size(11.0)
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(self.i18n.tr("rack.plugin_not_loaded"))
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                    });
                });
            return;
        }

        let param_infos = self.audio_engine.get_vst_node_parameters(node_id);

        Frame::group(ui.style())
            .fill(crate::ui::theme::BG_PANEL)
            .inner_margin(12.0)
            .corner_radius(CornerRadius::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(self.i18n.tr("rack.plugin_editor"))
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.label(
                    RichText::new(&node_name)
                        .size(13.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.add_space(4.0);

                let knob_size = 44.0;
                let params_per_row = 3;

                for chunk in param_infos.chunks(params_per_row) {
                    ui.horizontal_wrapped(|ui| {
                        for (j, _param) in chunk.iter().enumerate() {
                            let param_idx = (chunk.as_ptr() as usize
                                - param_infos.as_ptr() as usize)
                                / std::mem::size_of::<crate::audio::chain::ParamInfo>()
                                + j;
                            let mut value = self
                                .audio_engine
                                .get_vst_node_parameter_value(node_id, param_idx);

                            ui.vertical(|ui| {
                                crate::ui::controls::draw_knob(
                                    ui,
                                    &mut value,
                                    &chunk[j].name,
                                    0.0,
                                    1.0,
                                    knob_size,
                                );
                            });

                            self.audio_engine
                                .set_vst_node_parameter(node_id, param_idx, value);
                        }
                    });
                }

                if param_infos.is_empty() {
                    ui.label(
                        RichText::new(self.i18n.tr("rack.no_parameters"))
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
            });
    }
}
