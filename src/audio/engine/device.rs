use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, HostId, SampleFormat};

use super::{AudioConfigInfo, AudioDeviceInfo, AudioEngine, AudioHostInfo};

pub fn enumerate_hosts() -> Vec<AudioHostInfo> {
    let default_host = cpal::default_host();
    let default_id = default_host.id();

    let mut hosts = vec![AudioHostInfo {
        id: default_id,
        name: format!("{:?}", default_id),
        is_default: true,
    }];

    for host_id in cpal::available_hosts() {
        if host_id == default_id {
            continue;
        }
        hosts.push(AudioHostInfo {
            id: host_id,
            name: format!("{:?}", host_id),
            is_default: false,
        });
    }

    hosts
}

pub(super) fn find_f32_output_config_range(
    device: &cpal::Device,
    preferred_sr: u32,
) -> Option<cpal::SupportedStreamConfigRange> {
    device
        .supported_output_configs()
        .ok()
        .and_then(|mut configs| {
            configs.find(|c| {
                c.sample_format() == SampleFormat::F32
                    && c.min_sample_rate().0 <= preferred_sr
                    && c.max_sample_rate().0 >= preferred_sr
            })
        })
        .or_else(|| {
            device
                .supported_output_configs()
                .ok()?
                .find(|c| c.sample_format() == SampleFormat::F32)
        })
}

pub(super) fn find_f32_input_config_range(
    device: &cpal::Device,
    preferred_sr: u32,
) -> Option<cpal::SupportedStreamConfigRange> {
    device
        .supported_input_configs()
        .ok()
        .and_then(|mut configs| {
            configs.find(|c| {
                c.sample_format() == SampleFormat::F32
                    && c.min_sample_rate().0 <= preferred_sr
                    && c.max_sample_rate().0 >= preferred_sr
            })
        })
        .or_else(|| {
            device
                .supported_input_configs()
                .ok()?
                .find(|c| c.sample_format() == SampleFormat::F32)
        })
}

pub(super) fn find_f32_input_config_range_exact(
    device: &cpal::Device,
    sample_rate: u32,
) -> Option<cpal::SupportedStreamConfigRange> {
    device
        .supported_input_configs()
        .ok()
        .and_then(|mut configs| {
            configs.find(|c| {
                c.sample_format() == SampleFormat::F32
                    && c.min_sample_rate().0 <= sample_rate
                    && c.max_sample_rate().0 >= sample_rate
            })
        })
}

pub(super) fn config_from_range(
    range: cpal::SupportedStreamConfigRange,
    preferred_sr: u32,
    buffer_size: u32,
) -> cpal::StreamConfig {
    let sr = cpal::SampleRate(
        preferred_sr
            .max(range.min_sample_rate().0)
            .min(range.max_sample_rate().0),
    );
    let mut config = range.with_sample_rate(sr).config();
    config.buffer_size = BufferSize::Fixed(buffer_size);
    config
}

impl AudioEngine {
    pub fn enumerate_devices(
        host_id: Option<HostId>,
    ) -> (Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>) {
        let host = host_id
            .and_then(|id| cpal::host_from_id(id).ok())
            .unwrap_or_else(cpal::default_host);

        let default_input = host.default_input_device().and_then(|d| d.name().ok());
        let default_output = host.default_output_device().and_then(|d| d.name().ok());

        let inputs: Vec<AudioDeviceInfo> = match host.input_devices() {
            Ok(devices) => devices
                .filter_map(|d| {
                    let name = d.name().ok()?;
                    Some(AudioDeviceInfo {
                        is_default: default_input.as_ref() == Some(&name),
                        name,
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        let outputs: Vec<AudioDeviceInfo> = match host.output_devices() {
            Ok(devices) => devices
                .filter_map(|d| {
                    let name = d.name().ok()?;
                    Some(AudioDeviceInfo {
                        is_default: default_output.as_ref() == Some(&name),
                        name,
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        (inputs, outputs)
    }

    pub fn get_supported_config(
        host_id: Option<HostId>,
        device_name: &str,
        is_input: bool,
    ) -> Option<AudioConfigInfo> {
        let host = host_id
            .and_then(|id| cpal::host_from_id(id).ok())
            .unwrap_or_else(cpal::default_host);

        let device = if is_input {
            host.input_devices()
                .ok()?
                .find(|d| d.name().ok() == Some(device_name.to_string()))?
        } else {
            host.output_devices()
                .ok()?
                .find(|d| d.name().ok() == Some(device_name.to_string()))?
        };

        let default_cfg = if is_input {
            device.default_input_config().ok()?
        } else {
            device.default_output_config().ok()?
        };

        let default_sr = default_cfg.sample_rate().0;
        let default_bs = match default_cfg.buffer_size() {
            cpal::SupportedBufferSize::Range { min, .. } => Some(*min),
            cpal::SupportedBufferSize::Unknown => None,
        };

        let mut sample_rates: Vec<u32> = Vec::new();
        let common_rates: &[u32] = &[44100, 48000, 88200, 96000, 176400, 192000];

        if is_input {
            if let Ok(configs) = device.supported_input_configs() {
                for c in configs {
                    if c.sample_format() == SampleFormat::F32 {
                        let min_sr = c.min_sample_rate().0;
                        let max_sr = c.max_sample_rate().0;
                        for &sr in common_rates {
                            if sr >= min_sr && sr <= max_sr && !sample_rates.contains(&sr) {
                                sample_rates.push(sr);
                            }
                        }
                    }
                }
            }
        } else {
            if let Ok(configs) = device.supported_output_configs() {
                for c in configs {
                    if c.sample_format() == SampleFormat::F32 {
                        let min_sr = c.min_sample_rate().0;
                        let max_sr = c.max_sample_rate().0;
                        for &sr in common_rates {
                            if sr >= min_sr && sr <= max_sr && !sample_rates.contains(&sr) {
                                sample_rates.push(sr);
                            }
                        }
                    }
                }
            }
        }

        sample_rates.sort();

        if sample_rates.is_empty() {
            sample_rates = vec![44100, 48000, 96000];
        }

        let buffer_sizes = vec![32, 64, 128, 256, 512, 1024, 2048];

        Some(AudioConfigInfo {
            sample_rates,
            buffer_sizes,
            default_sample_rate: Some(default_sr),
            default_buffer_size: default_bs,
        })
    }

    pub fn get_supported_output_config_for_io(
        host_id: Option<HostId>,
        output_device_name: &str,
        input_device_name: Option<&str>,
    ) -> Option<AudioConfigInfo> {
        let mut cfg = Self::get_supported_config(host_id, output_device_name, false)?;
        let Some(input_device_name) = input_device_name else {
            return Some(cfg);
        };

        let input_cfg = Self::get_supported_config(host_id, input_device_name, true)?;
        cfg.sample_rates
            .retain(|sr| input_cfg.sample_rates.iter().any(|input_sr| input_sr == sr));

        if !cfg
            .sample_rates
            .contains(&cfg.default_sample_rate.unwrap_or_default())
        {
            cfg.default_sample_rate = cfg.sample_rates.last().copied();
        }

        Some(cfg)
    }

    pub fn get_device_channels(
        host_id: Option<HostId>,
        device_name: &str,
        is_input: bool,
    ) -> Option<u16> {
        let host = host_id
            .and_then(|id| cpal::host_from_id(id).ok())
            .unwrap_or_else(cpal::default_host);

        let device = if is_input {
            host.input_devices()
                .ok()?
                .find(|d| d.name().ok() == Some(device_name.to_string()))?
        } else {
            host.output_devices()
                .ok()?
                .find(|d| d.name().ok() == Some(device_name.to_string()))?
        };

        let cfg = if is_input {
            device.default_input_config().ok()?
        } else {
            device.default_output_config().ok()?
        };

        Some(cfg.channels())
    }

    pub fn current_input_device_name(&self) -> &Option<String> {
        &self.input_device_name
    }

    pub fn current_output_device_name(&self) -> &Option<String> {
        &self.output_device_name
    }

    pub fn restart_with_config(
        &mut self,
        host_id: Option<HostId>,
        input_name: Option<&str>,
        output_name: Option<&str>,
        sample_rate: u32,
        buffer_size: u32,
        input_ch: usize,
        output_ch: (usize, usize),
    ) -> anyhow::Result<()> {
        self.stop();

        self.host_id = host_id;
        self.input_device_name = input_name.map(String::from);
        self.output_device_name = output_name.map(String::from);
        self.sample_rate = sample_rate as f64;
        self.buffer_size = buffer_size;
        self.input_channel = input_ch;
        self.output_channels = output_ch;

        self.start()
    }
}
