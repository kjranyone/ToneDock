use egui::*;

use crate::audio::engine::{enumerate_hosts, AudioConfigInfo, AudioDeviceInfo, AudioHostInfo};
use crate::i18n::I18n;

use super::{PreferencesResult, SZ_BODY, SZ_SECTION, SZ_SMALL};

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
    pub output_channel_count: u16,
    pub input_channel_count: u16,
    pub output_ch_l: usize,
    pub output_ch_r: usize,
    pub input_ch: usize,
    output_changed: bool,
    input_changed: bool,
    host_changed: bool,
}

impl AudioSettingsState {
    pub fn new(
        current_host: Option<cpal::HostId>,
        current_sr: u32,
        current_bs: u32,
        current_input_ch: usize,
        current_output_ch: (usize, usize),
    ) -> Self {
        let hosts = enumerate_hosts();
        let selected_host = current_host.or_else(|| hosts.first().map(|h| h.id));

        let (inputs, outputs) = crate::audio::engine::AudioEngine::enumerate_devices(selected_host);

        let default_input_idx = inputs.iter().position(|d| d.is_default);
        let default_output_idx = outputs.iter().position(|d| d.is_default);

        let output_name = default_output_idx.map(|i| outputs[i].name.as_str());
        let input_name = default_input_idx.map(|i| inputs[i].name.as_str());
        let config_info = output_name.and_then(|n| {
            crate::audio::engine::AudioEngine::get_supported_output_config_for_io(
                selected_host,
                n,
                input_name,
            )
        });

        let output_channel_count = default_output_idx
            .and_then(|i| {
                crate::audio::engine::AudioEngine::get_device_channels(
                    selected_host,
                    &outputs[i].name,
                    false,
                )
            })
            .unwrap_or(2);

        let input_channel_count = default_input_idx
            .and_then(|i| {
                crate::audio::engine::AudioEngine::get_device_channels(
                    selected_host,
                    &inputs[i].name,
                    true,
                )
            })
            .unwrap_or(2);

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
            output_channel_count,
            input_channel_count,
            output_ch_l: current_output_ch.0.min(output_channel_count as usize - 1),
            output_ch_r: current_output_ch.1.min(output_channel_count as usize - 1),
            input_ch: current_input_ch.min(input_channel_count as usize - 1),
            output_changed: false,
            input_changed: false,
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
        self.refresh_output_channels();
        self.refresh_input_channels();
        self.refresh_config();
    }

    fn refresh_output_channels(&mut self) {
        self.output_channel_count = self
            .selected_output
            .and_then(|i| {
                crate::audio::engine::AudioEngine::get_device_channels(
                    self.selected_host,
                    &self.output_devices[i].name,
                    false,
                )
            })
            .unwrap_or(2);
        self.output_ch_l = self.output_ch_l.min(self.output_channel_count as usize - 1);
        self.output_ch_r = self.output_ch_r.min(self.output_channel_count as usize - 1);
    }

    fn refresh_input_channels(&mut self) {
        self.input_channel_count = self
            .selected_input
            .and_then(|i| {
                crate::audio::engine::AudioEngine::get_device_channels(
                    self.selected_host,
                    &self.input_devices[i].name,
                    true,
                )
            })
            .unwrap_or(2);
        self.input_ch = self.input_ch.min(self.input_channel_count as usize - 1);
    }

    fn refresh_config(&mut self) {
        let output_name = self
            .selected_output
            .and_then(|i| self.output_devices.get(i))
            .map(|d| d.name.clone());
        let input_name = self
            .selected_input
            .and_then(|i| self.input_devices.get(i))
            .map(|d| d.name.clone());
        self.config_info = output_name.as_deref().and_then(|n| {
            crate::audio::engine::AudioEngine::get_supported_output_config_for_io(
                self.selected_host,
                n,
                input_name.as_deref(),
            )
        });

        let is_asio = self.is_asio();

        if let Some(ref cfg) = self.config_info {
            if is_asio {
                if let Some(sr) = cfg.default_sample_rate {
                    self.sample_rate = sr;
                }
                if let Some(bs) = cfg.default_buffer_size {
                    self.buffer_size = bs;
                }
            } else {
                if !cfg.sample_rates.contains(&self.sample_rate) {
                    self.sample_rate = cfg.sample_rates.last().copied().unwrap_or(48000);
                }
                if !cfg.buffer_sizes.contains(&self.buffer_size) {
                    self.buffer_size = 256;
                }
            }
        }
    }

    pub fn is_asio(&self) -> bool {
        self.selected_host
            .map(|id| {
                #[cfg(feature = "asio")]
                {
                    id == cpal::HostId::Asio
                }
                #[cfg(not(feature = "asio"))]
                {
                    false
                }
            })
            .unwrap_or(false)
    }
}

pub(super) fn show_audio_tab(
    ui: &mut Ui,
    state: &mut AudioSettingsState,
    i18n: &I18n,
) -> PreferencesResult {
    let mut result = PreferencesResult::None;

    ui.label(
        RichText::new(i18n.tr("prefs.audio_driver"))
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(2.0);

    let default_suffix = i18n.tr("prefs.default_suffix");

    let selected_host_name = state
        .selected_host
        .and_then(|id| state.hosts.iter().find(|h| h.id == id))
        .map(|h| {
            if h.is_default {
                format!("{}{}", h.name, default_suffix)
            } else {
                h.name.clone()
            }
        })
        .unwrap_or_else(|| i18n.tr("prefs.none").into());

    let host_ids: Vec<cpal::HostId> = state.hosts.iter().map(|h| h.id).collect();

    egui::ComboBox::from_id_salt("audio_host")
        .selected_text(&selected_host_name)
        .show_ui(ui, |ui| {
            for (i, host) in state.hosts.iter().enumerate() {
                let label = if host.is_default {
                    format!("{}{}", host.name, default_suffix)
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

    ui.add_space(10.0);

    ui.label(
        RichText::new(i18n.tr("prefs.output_device"))
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(2.0);

    let output_names: Vec<String> = state
        .output_devices
        .iter()
        .map(|d| {
            if d.is_default {
                format!("{}{}", d.name, default_suffix)
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
                .unwrap_or(i18n.tr("prefs.none")),
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
        state.refresh_output_channels();
        state.refresh_config();
    }

    if state.output_channel_count > 0 {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n.trf(
                    "prefs.channels",
                    &[("count", &state.output_channel_count.to_string())],
                ))
                .size(SZ_SMALL)
                .color(crate::ui::theme::TEXT_SECONDARY),
            );
        });
        ui.horizontal(|ui| {
            ui.label(i18n.tr("prefs.left_ch"));
            let mut ch_l = state.output_ch_l as u32;
            egui::DragValue::new(&mut ch_l)
                .range(0..=state.output_channel_count as u32 - 1)
                .speed(0.1)
                .ui(ui);
            state.output_ch_l = ch_l as usize;

            ui.add_space(8.0);

            ui.label(i18n.tr("prefs.right_ch"));
            let mut ch_r = state.output_ch_r as u32;
            egui::DragValue::new(&mut ch_r)
                .range(0..=state.output_channel_count as u32 - 1)
                .speed(0.1)
                .ui(ui);
            state.output_ch_r = ch_r as usize;
        });
    }

    ui.add_space(10.0);

    ui.label(
        RichText::new(i18n.tr("prefs.input_device"))
            .size(SZ_SECTION)
            .color(crate::ui::theme::ACCENT),
    );
    ui.add_space(2.0);

    let input_names: Vec<String> = state
        .input_devices
        .iter()
        .map(|d| {
            if d.is_default {
                format!("{}{}", d.name, default_suffix)
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
                .unwrap_or(i18n.tr("prefs.none")),
        )
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(state.selected_input.is_none(), i18n.tr("prefs.none"))
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
                    state.input_changed = true;
                }
            }
        });

    if state.input_changed {
        state.input_changed = false;
        state.refresh_input_channels();
    }

    if state.input_channel_count > 0 {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(i18n.trf(
                    "prefs.channels",
                    &[("count", &state.input_channel_count.to_string())],
                ))
                .size(SZ_SMALL)
                .color(crate::ui::theme::TEXT_SECONDARY),
            );
        });
        ui.horizontal(|ui| {
            ui.label(i18n.tr("prefs.input_ch"));
            let mut input_ch = state.input_ch as u32;
            egui::DragValue::new(&mut input_ch)
                .range(0..=state.input_channel_count as u32 - 1)
                .speed(0.1)
                .ui(ui);
            state.input_ch = input_ch as usize;
        });
    }

    ui.add_space(10.0);

    let is_asio = state.is_asio();

    if is_asio {
        ui.label(
            RichText::new(i18n.tr("prefs.sample_rate_asio"))
                .size(SZ_SECTION)
                .color(crate::ui::theme::ACCENT),
        );
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!("{} Hz", state.sample_rate))
                .size(SZ_BODY)
                .color(crate::ui::theme::TEXT_PRIMARY),
        );

        ui.add_space(10.0);

        ui.label(
            RichText::new(i18n.tr("prefs.buffer_size_asio"))
                .size(SZ_SECTION)
                .color(crate::ui::theme::ACCENT),
        );
        ui.add_space(2.0);
        let latency_ms = state.buffer_size as f64 / state.sample_rate as f64 * 1000.0;
        ui.label(
            RichText::new(format!("{} ({:.1} ms)", state.buffer_size, latency_ms))
                .size(SZ_BODY)
                .color(crate::ui::theme::TEXT_PRIMARY),
        );
    } else {
        ui.label(
            RichText::new(i18n.tr("prefs.sample_rate"))
                .size(SZ_SECTION)
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

        ui.add_space(10.0);

        ui.label(
            RichText::new(i18n.tr("prefs.buffer_size"))
                .size(SZ_SECTION)
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
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(i18n.tr("prefs.apply")).clicked() {
                let input_name = state
                    .selected_input
                    .and_then(|i| state.input_devices.get(i))
                    .map(|d| d.name.clone());
                let output_name = state
                    .selected_output
                    .and_then(|i| state.output_devices.get(i))
                    .map(|d| d.name.clone());

                result = PreferencesResult::AudioApply {
                    host_id: state.selected_host,
                    input_name,
                    output_name,
                    sample_rate: state.sample_rate,
                    buffer_size: state.buffer_size,
                    input_ch: state.input_ch,
                    output_ch: (state.output_ch_l, state.output_ch_r),
                };
            }

            if ui.button(i18n.tr("prefs.cancel")).clicked() {
                result = PreferencesResult::AudioCancel;
            }
        });
    });

    result
}
