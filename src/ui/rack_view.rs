use crate::audio::node::NodeId;
use crate::i18n::I18n;
use crate::vst_host::scanner::PluginInfo;
use egui::*;

pub struct RackSlotView {
    pub node_id: NodeId,
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub loaded: bool,
    pub enabled: bool,
    pub bypassed: bool,
    pub has_editor: bool,
    pub editor_open: bool,
    pub preferred_editor_size: (i32, i32),
}

pub enum RackCommand {
    Select(NodeId),
    Add(usize),
    Remove(NodeId),
    Reorder(NodeId, usize),
    ToggleBypass(NodeId),
    ToggleEnable(NodeId),
    OpenEditor(NodeId),
    CloseEditor(NodeId),
}

pub struct RackView {
    pub selected_plugin: Option<NodeId>,
    dragging_plugin: Option<NodeId>,
}

impl RackView {
    pub fn new() -> Self {
        Self {
            selected_plugin: None,
            dragging_plugin: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        rack_slots: &[RackSlotView],
        available_plugins: &[PluginInfo],
        inline_editor_node: Option<NodeId>,
        i18n: &I18n,
    ) -> (Vec<RackCommand>, Vec<(NodeId, Rect)>) {
        let mut commands = Vec::new();
        let mut inline_rects: Vec<(NodeId, Rect)> = Vec::new();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n.tr("rack.plugin_chain"))
                    .size(11.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(8.0);

            ui.menu_button(RichText::new(i18n.tr("rack.add_plugin")).size(12.0), |ui| {
                if available_plugins.is_empty() {
                    ui.label(
                        RichText::new(i18n.tr("rack.no_plugins_hint"))
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
                for (i, plugin) in available_plugins.iter().enumerate() {
                    if ui.button(&plugin.name).clicked() {
                        commands.push(RackCommand::Add(i));
                        ui.close_menu();
                    }
                }
            });
        });

        ui.add_space(6.0);

        ScrollArea::vertical()
            .auto_shrink([false, true])
            .max_height(ui.available_height())
            .show(ui, |ui| {
                let slots_count = rack_slots.len();
                let mut slot_rects: Vec<(NodeId, Rect)> = Vec::with_capacity(slots_count);

                for (i, slot) in rack_slots.iter().enumerate() {
                    let bg = if !slot.enabled {
                        crate::ui::theme::DISABLED
                    } else if slot.bypassed {
                        crate::ui::theme::BYPASSED
                    } else {
                        crate::ui::theme::SURFACE_CONTAINER_HIGH
                    };

                    let is_selected = self.selected_plugin == Some(slot.node_id);
                    let border_color = if is_selected {
                        crate::ui::theme::ACCENT
                    } else {
                        crate::ui::theme::OUTLINE_VAR
                    };

                    let frame = Frame::group(ui.style())
                        .fill(bg)
                        .stroke(Stroke::new(
                            if is_selected { 2.0 } else { 1.0 },
                            border_color,
                        ))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(12.0)
                        .shadow(Shadow {
                            offset: [0, 4],
                            blur: 16,
                            spread: 0,
                            color: Color32::from_black_alpha(if is_selected { 70 } else { 38 }),
                        });

                    let is_inline_target = inline_editor_node == Some(slot.node_id);

                    let response = frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let grip = ui.add(
                                Label::new(RichText::new("≡").size(18.0)).sense(Sense::drag()),
                            );
                            if grip.drag_started() {
                                self.dragging_plugin = Some(slot.node_id);
                            }

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    let led = if !slot.enabled {
                                        crate::ui::theme::OUTLINE
                                    } else if slot.bypassed {
                                        crate::ui::theme::ACCENT_WARM
                                    } else {
                                        crate::ui::theme::METER_GREEN
                                    };
                                    let (led_rect, _) =
                                        ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                    ui.painter().circle_filled(led_rect.center(), 4.0, led);
                                    ui.painter().circle_filled(
                                        led_rect.center(),
                                        7.0,
                                        Color32::from_rgba_unmultiplied(
                                            led.r(),
                                            led.g(),
                                            led.b(),
                                            30,
                                        ),
                                    );
                                    ui.label(
                                        RichText::new(&slot.name)
                                            .size(14.0)
                                            .strong()
                                            .color(crate::ui::theme::TEXT_PRIMARY),
                                    );
                                });
                                if !slot.vendor.is_empty() || !slot.category.is_empty() {
                                    let detail =
                                        if !slot.vendor.is_empty() && !slot.category.is_empty() {
                                            format!("{} / {}", slot.vendor, slot.category)
                                        } else {
                                            format!("{}{}", slot.vendor, slot.category)
                                        };
                                    ui.label(
                                        RichText::new(detail)
                                            .size(9.0)
                                            .color(crate::ui::theme::TEXT_SECONDARY),
                                    );
                                }
                                if slot.loaded {
                                    ui.label(
                                        RichText::new(i18n.tr("rack.loaded"))
                                            .size(9.0)
                                            .color(crate::ui::theme::METER_GREEN),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new(i18n.tr("rack.not_loaded"))
                                            .size(9.0)
                                            .color(crate::ui::theme::METER_RED),
                                    );
                                }
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button(RichText::new("X").size(11.0)).clicked() {
                                    commands.push(RackCommand::Remove(slot.node_id));
                                }
                                let bypass_text = if slot.bypassed {
                                    i18n.tr("rack.unbypass")
                                } else {
                                    i18n.tr("rack.bypass")
                                };
                                if ui.button(RichText::new(bypass_text).size(11.0)).clicked() {
                                    commands.push(RackCommand::ToggleBypass(slot.node_id));
                                }
                                let enable_text = if slot.enabled {
                                    i18n.tr("rack.disable")
                                } else {
                                    i18n.tr("rack.enable")
                                };
                                if ui.button(RichText::new(enable_text).size(11.0)).clicked() {
                                    commands.push(RackCommand::ToggleEnable(slot.node_id));
                                }

                                if slot.has_editor {
                                    if slot.editor_open {
                                        if ui.button(i18n.tr("rack.close_gui")).clicked() {
                                            commands.push(RackCommand::CloseEditor(slot.node_id));
                                        }
                                    } else if ui.button(i18n.tr("rack.open_gui")).clicked() {
                                        commands.push(RackCommand::OpenEditor(slot.node_id));
                                    }
                                }
                            });
                        });

                        if is_inline_target {
                            ui.add_space(8.0);
                            let available_width = ui.available_width().max(280.0);
                            let scale = ui.ctx().pixels_per_point();
                            let preferred_h = slot.preferred_editor_size.1 as f32 / scale;
                            let height = preferred_h.clamp(200.0, 600.0);
                            let (rect, _) = ui
                                .allocate_exact_size(vec2(available_width, height), Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(10),
                                Color32::from_rgb(10, 10, 12),
                            );
                            inline_rects.push((slot.node_id, rect));
                        }

                        ui.add_space(4.0);
                        let rack_line = ui.max_rect().shrink2(vec2(2.0, 0.0));
                        ui.painter().line_segment(
                            [
                                pos2(rack_line.left(), rack_line.bottom() - 2.0),
                                pos2(rack_line.right(), rack_line.bottom() - 2.0),
                            ],
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 14)),
                        );
                    });

                    if response.response.clicked() {
                        self.selected_plugin = Some(slot.node_id);
                        commands.push(RackCommand::Select(slot.node_id));
                    }
                    slot_rects.push((slot.node_id, response.response.rect));

                    if i < slots_count - 1 {
                        ui.vertical_centered(|ui| {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new("\u{25BE}")
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                            ui.add_space(2.0);
                        });
                    }
                }

                if let Some(dragged_node) = self.dragging_plugin {
                    if let Some(pointer_pos) = ui.ctx().pointer_latest_pos() {
                        if let Some((_, rect)) = slot_rects
                            .iter()
                            .find(|(_, rect)| rect.contains(pointer_pos))
                        {
                            ui.painter().rect_stroke(
                                rect.expand(4.0),
                                CornerRadius::same(16),
                                Stroke::new(2.0, crate::ui::theme::ACCENT),
                                StrokeKind::Outside,
                            );
                        }
                    }

                    let released = ui.input(|i| i.pointer.any_released());
                    if released {
                        if let Some(pointer_pos) = ui.ctx().pointer_latest_pos() {
                            if let Some((target_index, _)) = slot_rects
                                .iter()
                                .enumerate()
                                .find(|(_, (_, rect))| rect.contains(pointer_pos))
                            {
                                commands.push(RackCommand::Reorder(dragged_node, target_index));
                            }
                        }
                        self.dragging_plugin = None;
                    }
                }

                if rack_slots.is_empty() {
                    Frame::group(ui.style())
                        .fill(crate::ui::theme::SURFACE_CONTAINER)
                        .corner_radius(CornerRadius::same(18))
                        .inner_margin(30.0)
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(i18n.tr("rack.no_rack_plugins"))
                                        .size(16.0)
                                        .color(crate::ui::theme::TEXT_SECONDARY),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(i18n.tr("rack.empty_hint"))
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_HINT),
                                );
                            });
                        });
                }
            });

        (commands, inline_rects)
    }
}
