use crate::vst_host::scanner::PluginInfo;
use egui::*;

pub enum RackCommand {
    Select(usize),
    Add(usize),
    Remove(usize),
    Move(usize, usize),
    ToggleBypass(usize),
    ToggleEnable(usize),
    OpenEditor(usize),
    CloseEditor(usize),
}

pub struct RackView {
    pub selected_plugin: Option<usize>,
}

impl RackView {
    pub fn new() -> Self {
        Self {
            selected_plugin: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        chain_slots: &mut Vec<crate::audio::chain::PluginSlot>,
        available_plugins: &[PluginInfo],
    ) -> Vec<RackCommand> {
        let mut commands = Vec::new();

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("PLUGIN CHAIN")
                        .size(11.0)
                        .color(crate::ui::theme::ACCENT),
                );
                ui.add_space(8.0);

                ui.menu_button(RichText::new("+ Add Plugin").size(12.0), |ui| {
                    if available_plugins.is_empty() {
                        ui.label(
                            RichText::new(
                                "No plugins found.\nOpen Settings → Plugins\nto add VST3 paths.",
                            )
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

            let mut remove_idx = None;
            let mut move_from_to: Option<(usize, usize)> = None;
            let slots_count = chain_slots.len();

            for (i, slot) in chain_slots.iter_mut().enumerate() {
                let bg = if !slot.enabled {
                    crate::ui::theme::DISABLED
                } else if slot.bypassed {
                    crate::ui::theme::BYPASSED
                } else {
                    crate::ui::theme::SURFACE_CONTAINER_HIGH
                };

                let is_selected = self.selected_plugin == Some(i);

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

                let response = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
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
                                    Color32::from_rgba_unmultiplied(led.r(), led.g(), led.b(), 30),
                                );
                                ui.label(
                                    RichText::new(&slot.info.name)
                                        .size(14.0)
                                        .strong()
                                        .color(crate::ui::theme::TEXT_PRIMARY),
                                );
                            });
                            let vendor = &slot.info.vendor;
                            let category = &slot.info.category;
                            if !vendor.is_empty() || !category.is_empty() {
                                let detail = if !vendor.is_empty() && !category.is_empty() {
                                    format!("{} / {}", vendor, category)
                                } else {
                                    format!("{}{}", vendor, category)
                                };
                                ui.label(
                                    RichText::new(detail)
                                        .size(9.0)
                                        .color(crate::ui::theme::TEXT_SECONDARY),
                                );
                            }
                            if let Some(ref _instance) = slot.instance {
                                ui.label(
                                    RichText::new("Loaded")
                                        .size(9.0)
                                        .color(crate::ui::theme::METER_GREEN),
                                );
                            } else {
                                ui.label(
                                    RichText::new("Not loaded")
                                        .size(9.0)
                                        .color(crate::ui::theme::METER_RED),
                                );
                            }
                        });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(RichText::new("X").size(11.0)).clicked() {
                                remove_idx = Some(i);
                            }
                            if i > 0 && ui.button(RichText::new("Up").size(11.0)).clicked() {
                                move_from_to = Some((i, i - 1));
                            }
                            if i < slots_count - 1
                                && ui.button(RichText::new("Down").size(11.0)).clicked()
                            {
                                move_from_to = Some((i, i + 1));
                            }
                            let bypass_text = if slot.bypassed { "Unbypass" } else { "Bypass" };
                            if ui.button(RichText::new(bypass_text).size(11.0)).clicked() {
                                commands.push(RackCommand::ToggleBypass(i));
                            }
                            let enable_text = if slot.enabled { "Disable" } else { "Enable" };
                            if ui.button(RichText::new(enable_text).size(11.0)).clicked() {
                                commands.push(RackCommand::ToggleEnable(i));
                            }

                            if let Some(ref instance) = slot.instance {
                                if instance.has_editor() {
                                    if slot.editor.is_open() {
                                        if ui.button("Close GUI").clicked() {
                                            commands.push(RackCommand::CloseEditor(i));
                                        }
                                    } else {
                                        if ui.button("Open GUI").clicked() {
                                            commands.push(RackCommand::OpenEditor(i));
                                        }
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(8.0);
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
                    self.selected_plugin = Some(i);
                    commands.push(RackCommand::Select(i));
                }

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

            if chain_slots.is_empty() {
                Frame::group(ui.style())
                    .fill(crate::ui::theme::SURFACE_CONTAINER)
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(30.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No plugins loaded")
                                    .size(16.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Load plugins to build a hardware-style digital rack",
                                )
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_HINT),
                            );
                        });
                    });
            }

            if let Some(idx) = remove_idx {
                commands.push(RackCommand::Remove(idx));
            }
            if let Some((from, to)) = move_from_to {
                commands.push(RackCommand::Move(from, to));
            }
        });

        commands
    }
}
