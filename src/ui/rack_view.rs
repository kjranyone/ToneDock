use crate::vst_host::scanner::PluginInfo;
use egui::*;

pub enum RackCommand {
    Add(usize),
    Remove(usize),
    Move(usize, usize),
    ToggleBypass(usize),
    ToggleEnable(usize),
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
                        .size(12.0)
                        .color(crate::ui::theme::ACCENT),
                );
                ui.add_space(8.0);

                ui.menu_button("+ Add Plugin", |ui| {
                    for (i, plugin) in available_plugins.iter().enumerate() {
                        if ui.button(&plugin.name).clicked() {
                            commands.push(RackCommand::Add(i));
                            ui.close_menu();
                        }
                    }
                });
            });

            ui.add_space(4.0);

            let mut remove_idx = None;
            let mut move_from_to: Option<(usize, usize)> = None;
            let slots_count = chain_slots.len();

            for (i, slot) in chain_slots.iter_mut().enumerate() {
                let bg = if !slot.enabled {
                    crate::ui::theme::DISABLED
                } else if slot.bypassed {
                    crate::ui::theme::BYPASSED
                } else {
                    crate::ui::theme::BG_SLOT
                };

                let is_selected = self.selected_plugin == Some(i);

                Frame::group(ui.style())
                    .fill(bg)
                    .stroke(Stroke::new(
                        1.0,
                        if is_selected {
                            crate::ui::theme::ACCENT
                        } else {
                            egui::Color32::TRANSPARENT
                        },
                    ))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&slot.info.name)
                                        .size(11.0)
                                        .color(crate::ui::theme::TEXT_PRIMARY),
                                );
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
                                            .color(crate::ui::theme::TEXT_SECONDARY),
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
                                if ui.button("X").clicked() {
                                    remove_idx = Some(i);
                                }
                                if i > 0 && ui.button("Up").clicked() {
                                    move_from_to = Some((i, i - 1));
                                }
                                if i < slots_count - 1 && ui.button("Down").clicked() {
                                    move_from_to = Some((i, i + 1));
                                }
                                let bypass_text = if slot.bypassed { "Unbypass" } else { "Bypass" };
                                if ui.button(bypass_text).clicked() {
                                    commands.push(RackCommand::ToggleBypass(i));
                                }
                                let enable_text = if slot.enabled { "Disable" } else { "Enable" };
                                if ui.button(enable_text).clicked() {
                                    commands.push(RackCommand::ToggleEnable(i));
                                }
                            });
                        });
                    });

                if i < slots_count - 1 {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("▼")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                    });
                }
            }

            if chain_slots.is_empty() {
                Frame::group(ui.style())
                    .fill(crate::ui::theme::BG_PANEL)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("No plugins loaded")
                                    .size(12.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                            ui.label(
                                RichText::new("Click '+ Add Plugin' to get started")
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
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
