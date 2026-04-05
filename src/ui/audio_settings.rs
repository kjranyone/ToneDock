use egui::*;

use crate::audio::engine::{enumerate_hosts, AudioConfigInfo, AudioDeviceInfo, AudioHostInfo};

pub struct AudioSettingsState {
    pub hosts: Vec<AudioHostInfo>,
    pub selected_host: Option<cpal::HostId>,
    pub input_devices: Vec<AudioDeviceInfo>,
    pub output_devices: Vec<AudioDeviceInfo>,
    pub selected_input: Option<usize>,
    pub selected_output: Option<usize>,
    pub config_info: Option<AudioConfigInfo>,
    pub sample_rate: u32,
    pub buffer_size: u32,
    output_changed: bool,
    host_changed: bool,
}

impl AudioSettingsState {
    pub fn new(current_host: Option<cpal::HostId>, current_sr: u32, current_bs: u32) -> Self {
        let hosts = enumerate_hosts();
        let selected_host = current_host.or_else(|| hosts.first().map(|h| h.id));

        let (inputs, outputs) = crate::audio::engine::AudioEngine::enumerate_devices(selected_host);

        let default_input_idx = inputs.iter().position(|d| d.is_default);
        let default_output_idx = outputs.iter().position(|d| d.is_default);

        let output_name = default_output_idx.map(|i| outputs[i].name.as_str());
        let config_info = output_name.and_then(|n| {
            crate::audio::engine::AudioEngine::get_supported_config(selected_host, n, false)
        });

        Self {
            hosts,
            selected_host,
            input_devices: inputs,
            output_devices: outputs,
            selected_input: default_input_idx,
            selected_output: default_output_idx,
            config_info,
            sample_rate: current_sr,
            buffer_size: current_bs,
            output_changed: false,
            host_changed: false,
        }
    }

    fn refresh_devices(&mut self) {
        let (inputs, outputs) =
            crate::audio::engine::AudioEngine::enumerate_devices(self.selected_host);

        let default_input_idx = inputs.iter().position(|d| d.is_default);
        let default_output_idx = outputs.iter().position(|d| d.is_default);

        self.input_devices = inputs;
        self.output_devices = outputs;
        self.selected_input = default_input_idx;
        self.selected_output = default_output_idx;
        self.refresh_config();
    }

    fn refresh_config(&mut self) {
        let output_name = self
            .selected_output
            .and_then(|i| self.output_devices.get(i))
            .map(|d| d.name.clone());
        self.config_info = output_name.as_deref().and_then(|n| {
            crate::audio::engine::AudioEngine::get_supported_config(self.selected_host, n, false)
        });

        if let Some(ref cfg) = self.config_info {
            if !cfg.sample_rates.contains(&self.sample_rate) {
                self.sample_rate = cfg.sample_rates.last().copied().unwrap_or(48000);
            }
            if !cfg.buffer_sizes.contains(&self.buffer_size) {
                self.buffer_size = 256;
            }
        }
    }
}

pub fn show_audio_settings(
    ctx: &Context,
    state: &mut AudioSettingsState,
) -> Option<(
    Option<cpal::HostId>,
    Option<String>,
    Option<String>,
    u32,
    u32,
)> {
    let mut result = None;

    Window::new("Audio Settings")
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("AUDIO DRIVER")
                    .size(10.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(2.0);

            let selected_host_name = state
                .selected_host
                .and_then(|id| state.hosts.iter().find(|h| h.id == id))
                .map(|h| {
                    if h.is_default {
                        format!("{} (default)", h.name)
                    } else {
                        h.name.clone()
                    }
                })
                .unwrap_or_else(|| "(none)".into());

            let host_ids: Vec<cpal::HostId> = state.hosts.iter().map(|h| h.id).collect();

            egui::ComboBox::from_id_salt("audio_host")
                .selected_text(&selected_host_name)
                .show_ui(ui, |ui| {
                    for (i, host) in state.hosts.iter().enumerate() {
                        let label = if host.is_default {
                            format!("{} (default)", host.name)
                        } else {
                            host.name.clone()
                        };
                        if ui
                            .selectable_label(state.selected_host == Some(host_ids[i]), &label)
                            .clicked()
                        {
                            state.selected_host = Some(host_ids[i]);
                            state.host_changed = true;
                        }
                    }
                });

            if state.host_changed {
                state.host_changed = false;
                state.refresh_devices();
            }

            ui.add_space(8.0);

            ui.label(
                RichText::new("OUTPUT DEVICE")
                    .size(10.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(2.0);

            let output_names: Vec<String> = state
                .output_devices
                .iter()
                .map(|d| {
                    if d.is_default {
                        format!("{} (default)", d.name)
                    } else {
                        d.name.clone()
                    }
                })
                .collect();

            egui::ComboBox::from_id_salt("output_device")
                .selected_text(
                    state
                        .selected_output
                        .and_then(|i| output_names.get(i))
                        .map(|s| s.as_str())
                        .unwrap_or("(none)"),
                )
                .show_ui(ui, |ui| {
                    for (i, label) in output_names.iter().enumerate() {
                        if ui
                            .selectable_label(state.selected_output == Some(i), label)
                            .clicked()
                        {
                            state.selected_output = Some(i);
                            state.output_changed = true;
                        }
                    }
                });

            if state.output_changed {
                state.output_changed = false;
                state.refresh_config();
            }

            ui.add_space(8.0);

            ui.label(
                RichText::new("INPUT DEVICE")
                    .size(10.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(2.0);

            let input_names: Vec<String> = state
                .input_devices
                .iter()
                .map(|d| {
                    if d.is_default {
                        format!("{} (default)", d.name)
                    } else {
                        d.name.clone()
                    }
                })
                .collect();

            egui::ComboBox::from_id_salt("input_device")
                .selected_text(
                    state
                        .selected_input
                        .and_then(|i| input_names.get(i))
                        .map(|s| s.as_str())
                        .unwrap_or("(none)"),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(state.selected_input.is_none(), "(none)")
                        .clicked()
                    {
                        state.selected_input = None;
                    }
                    for (i, label) in input_names.iter().enumerate() {
                        if ui
                            .selectable_label(state.selected_input == Some(i), label)
                            .clicked()
                        {
                            state.selected_input = Some(i);
                        }
                    }
                });

            ui.add_space(8.0);

            ui.label(
                RichText::new("SAMPLE RATE")
                    .size(10.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(2.0);

            let sr_options: Vec<u32> = state
                .config_info
                .as_ref()
                .map(|c| c.sample_rates.clone())
                .unwrap_or_else(|| vec![44100, 48000, 96000]);

            egui::ComboBox::from_id_salt("sample_rate")
                .selected_text(format!("{} Hz", state.sample_rate))
                .show_ui(ui, |ui| {
                    for &sr in &sr_options {
                        if ui
                            .selectable_label(state.sample_rate == sr, format!("{} Hz", sr))
                            .clicked()
                        {
                            state.sample_rate = sr;
                        }
                    }
                });

            ui.add_space(8.0);

            ui.label(
                RichText::new("BUFFER SIZE")
                    .size(10.0)
                    .color(crate::ui::theme::ACCENT),
            );
            ui.add_space(2.0);

            let bs_options: Vec<u32> = state
                .config_info
                .as_ref()
                .map(|c| c.buffer_sizes.clone())
                .unwrap_or_else(|| vec![64, 128, 256, 512, 1024, 2048]);

            let sr_f64 = state.sample_rate as f64;

            egui::ComboBox::from_id_salt("buffer_size")
                .selected_text(format!("{}", state.buffer_size))
                .show_ui(ui, |ui| {
                    for &bs in &bs_options {
                        let latency_ms = bs as f64 / sr_f64 * 1000.0;
                        if ui
                            .selectable_label(
                                state.buffer_size == bs,
                                format!("{} ({:.1} ms)", bs, latency_ms),
                            )
                            .clicked()
                        {
                            state.buffer_size = bs;
                        }
                    }
                });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Apply").clicked() {
                        let input_name = state
                            .selected_input
                            .and_then(|i| state.input_devices.get(i))
                            .map(|d| d.name.clone());
                        let output_name = state
                            .selected_output
                            .and_then(|i| state.output_devices.get(i))
                            .map(|d| d.name.clone());

                        result = Some((
                            state.selected_host,
                            input_name,
                            output_name,
                            state.sample_rate,
                            state.buffer_size,
                        ));
                    }

                    if ui.button("Cancel").clicked() {
                        result = Some((None, None, None, 0, 0));
                    }
                });
            });
        });

    result
}
