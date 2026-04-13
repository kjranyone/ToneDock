use egui::*;

use crate::i18n::I18n;
use crate::midi::{MidiAction, MidiInput, MidiMap, TriggerMode};

use super::{SZ_BODY, SZ_SECTION, SZ_SMALL};

pub struct MidiTabState {
    pub selected_device_index: Option<usize>,
    pub devices: Vec<crate::midi::MidiDeviceInfo>,
}

impl MidiTabState {
    pub fn new() -> Self {
        Self {
            selected_device_index: None,
            devices: MidiInput::enumerate_devices(),
        }
    }

    pub fn refresh_devices(&mut self) {
        self.devices = MidiInput::enumerate_devices();
    }
}

pub enum MidiTabResult {
    None,
    Connect(usize),
    Disconnect,
    Learn(MidiAction),
    ClearBinding(MidiAction),
    SetTriggerMode(MidiAction, TriggerMode),
    ClearAll,
}

pub fn show_midi_tab(
    ui: &mut Ui,
    state: &mut MidiTabState,
    midi_map: &mut MidiMap,
    midi_connected: bool,
    midi_learning: bool,
    midi_learn_target: Option<MidiAction>,
    i18n: &I18n,
) -> MidiTabResult {
    let mut result = MidiTabResult::None;

    ui.label(
        RichText::new(i18n.tr("prefs.midi_device"))
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(4.0);

    if state.devices.is_empty() {
        ui.label(
            RichText::new(i18n.tr("prefs.midi_no_devices"))
                .size(SZ_BODY)
                .color(crate::ui::theme::TEXT_SECONDARY),
        );
    } else {
        egui::Grid::new("midi_device_grid")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                let mut selected_name = String::new();
                egui::ComboBox::from_id_salt("midi_device_select")
                    .selected_text(if midi_connected {
                        i18n.tr("prefs.midi_connected")
                    } else {
                        i18n.tr("prefs.midi_disconnected")
                    })
                    .show_ui(ui, |ui| {
                        for (i, device) in state.devices.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    state.selected_device_index == Some(i),
                                    &device.name,
                                )
                                .clicked()
                            {
                                state.selected_device_index = Some(i);
                                selected_name = device.name.clone();
                            }
                        }
                    });

                if ui.button(i18n.tr("prefs.midi_connect")).clicked() {
                    if let Some(idx) = state.selected_device_index {
                        result = MidiTabResult::Connect(idx);
                    }
                }

                if ui.button(i18n.tr("prefs.midi_disconnect")).clicked() {
                    result = MidiTabResult::Disconnect;
                }

                ui.end_row();

                if ui.button(i18n.tr("prefs.midi_refresh")).clicked() {
                    state.refresh_devices();
                }
            });
    }

    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(i18n.tr("prefs.midi_map"))
                .size(SZ_SECTION)
                .color(crate::ui::theme::ACCENT),
        );
        ui.add_space(8.0);
        if ui.button(i18n.tr("prefs.midi_clear_all")).clicked() {
            result = MidiTabResult::ClearAll;
        }
    });
    ui.add_space(4.0);

    if midi_learning {
        if let Some(target) = midi_learn_target {
            ui.colored_label(
                crate::ui::theme::METER_YELLOW,
                i18n.trf("prefs.midi_learning", &[("action", target.label())]),
            );
        }
    }

    let available_height = ui.available_height();
    egui::ScrollArea::vertical()
        .max_height(available_height.max(100.0))
        .show(ui, |ui| {
            egui::Grid::new("midi_bindings_grid")
                .num_columns(4)
                .spacing([8.0, 3.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(i18n.tr("prefs.midi_action"))
                            .size(SZ_SMALL)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
                    ui.label(
                        RichText::new(i18n.tr("prefs.midi_binding"))
                            .size(SZ_SMALL)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
                    ui.label(
                        RichText::new(i18n.tr("prefs.midi_trigger_mode"))
                            .size(SZ_SMALL)
                            .color(crate::ui::theme::TEXT_HINT),
                    );
                    ui.label("");
                    ui.end_row();

                    for &action in MidiAction::all() {
                        ui.label(
                            RichText::new(action.label())
                                .size(SZ_BODY)
                                .color(crate::ui::theme::TEXT_PRIMARY),
                        );

                        let binding_display = if let Some(binding) = midi_map.find_binding(action) {
                            binding.key.display()
                        } else {
                            i18n.tr("prefs.midi_none").to_string()
                        };

                        ui.label(RichText::new(&binding_display).size(SZ_BODY).color(
                            if midi_map.find_binding(action).is_some() {
                                crate::ui::theme::METER_GREEN
                            } else {
                                crate::ui::theme::TEXT_SECONDARY
                            },
                        ));

                        let current_mode = midi_map
                            .find_binding(action)
                            .map(|b| b.mode)
                            .unwrap_or(TriggerMode::Toggle);

                        egui::ComboBox::from_id_salt(format!("trigger_mode_{:?}", action))
                            .selected_text(match current_mode {
                                TriggerMode::Toggle => i18n.tr("prefs.midi_toggle"),
                                TriggerMode::Momentary => i18n.tr("prefs.midi_momentary"),
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        current_mode == TriggerMode::Toggle,
                                        i18n.tr("prefs.midi_toggle"),
                                    )
                                    .clicked()
                                {
                                    result =
                                        MidiTabResult::SetTriggerMode(action, TriggerMode::Toggle);
                                }
                                if ui
                                    .selectable_label(
                                        current_mode == TriggerMode::Momentary,
                                        i18n.tr("prefs.midi_momentary"),
                                    )
                                    .clicked()
                                {
                                    result = MidiTabResult::SetTriggerMode(
                                        action,
                                        TriggerMode::Momentary,
                                    );
                                }
                            });

                        ui.horizontal(|ui| {
                            let is_learning = midi_learning && midi_learn_target == Some(action);
                            let learn_label = if is_learning {
                                "...".to_string()
                            } else {
                                i18n.tr("prefs.midi_learn").to_string()
                            };
                            let learn_fill = if is_learning {
                                crate::ui::theme::ACCENT
                            } else {
                                crate::ui::theme::SURFACE_CONTAINER_HIGH
                            };
                            if ui
                                .add(
                                    Button::new(RichText::new(&learn_label).size(SZ_SMALL))
                                        .fill(learn_fill)
                                        .min_size(Vec2::new(50.0, 20.0)),
                                )
                                .clicked()
                                && !is_learning
                            {
                                result = MidiTabResult::Learn(action);
                            }

                            if midi_map.find_binding(action).is_some() {
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new(i18n.tr("prefs.midi_clear_binding"))
                                                .size(SZ_SMALL),
                                        )
                                        .min_size(Vec2::new(50.0, 20.0)),
                                    )
                                    .clicked()
                                {
                                    result = MidiTabResult::ClearBinding(action);
                                }
                            }
                        });

                        ui.end_row();
                    }
                });
        });

    result
}
