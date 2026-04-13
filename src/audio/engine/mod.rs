mod backing_track;
mod device;
mod graph_commands;
mod helpers;
mod input_fifo;
mod serialization;
mod undo;

#[cfg(test)]
mod tests;

use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, HostId, Stream};
use parking_lot::Mutex;
use std::sync::Arc;

use crate::audio::graph::AudioGraph;
use crate::audio::graph_command::GraphCommand;
use crate::audio::node::{Connection, NodeId, NodeType, PortId};

pub use device::enumerate_hosts;

use device::{
    config_from_range, find_f32_input_config_range, find_f32_input_config_range_exact,
    find_f32_output_config_range,
};
use helpers::{apply_command, commit_and_publish_graph};
use input_fifo::InputFifo;

pub struct AudioHostInfo {
    pub id: HostId,
    pub name: String,
    pub is_default: bool,
}

pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub struct AudioConfigInfo {
    pub sample_rates: Vec<u32>,
    pub buffer_sizes: Vec<u32>,
    pub default_sample_rate: Option<u32>,
    pub default_buffer_size: Option<u32>,
}

pub struct AudioEngine {
    pub sample_rate: f64,
    pub buffer_size: u32,
    pub graph: Arc<ArcSwap<AudioGraph>>,
    pub master_volume: Arc<Mutex<f32>>,
    pub input_gain: Arc<Mutex<f32>>,
    pub output_level: Arc<Mutex<(f32, f32)>>,
    pub input_level: Arc<Mutex<(f32, f32)>>,
    pub cpu_usage: Arc<Mutex<f32>>,
    pub dropout_count: Arc<Mutex<u64>>,
    pub input_channel: usize,
    pub output_channels: (usize, usize),
    #[allow(dead_code)]
    pub chain_node_ids: Vec<NodeId>,
    pub input_node_id: NodeId,
    #[allow(dead_code)]
    pub output_node_id: NodeId,
    pub master_mixer_id: NodeId,

    pub metronome_node_id: Option<NodeId>,
    pub looper_node_id: Option<NodeId>,
    pub backing_track_node_id: Option<NodeId>,

    pub(crate) stream: Option<Stream>,
    pub(crate) input_stream: Option<Stream>,
    pub(crate) host_id: Option<HostId>,
    pub(crate) input_device_name: Option<String>,
    pub(crate) output_device_name: Option<String>,

    pub(crate) command_tx: crossbeam_channel::Sender<GraphCommand>,
    pub(crate) command_rx: crossbeam_channel::Receiver<GraphCommand>,
}

impl AudioEngine {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let host_id = host.id();

        let output_device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device found"))?;

        let output_range = find_f32_output_config_range(&output_device, 48000)
            .ok_or_else(|| anyhow::anyhow!("No f32 output config found"))?;

        let out_config = config_from_range(output_range, 48000, 256);
        let sample_rate = out_config.sample_rate.0 as f64;
        if sample_rate <= 0.0 {
            return Err(anyhow::anyhow!(
                "Invalid sample rate from device: {}",
                sample_rate
            ));
        }
        let buffer_size = match out_config.buffer_size {
            BufferSize::Fixed(bs) => bs,
            BufferSize::Default => 256,
        };
        let out_channels = out_config.channels as usize;
        if out_channels == 0 {
            return Err(anyhow::anyhow!("Output device has 0 channels"));
        }

        let in_channels = host
            .default_input_device()
            .as_ref()
            .and_then(|d| find_f32_input_config_range(d, sample_rate as u32))
            .map(|r| r.channels() as usize)
            .unwrap_or(0);

        let mut graph = AudioGraph::new(sample_rate, buffer_size as usize);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let master_mixer_id = graph.add_node(NodeType::Mixer { inputs: 1 }).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();
        let metronome_id = graph.add_node(NodeType::Metronome).unwrap();
        let looper_id = graph.add_node(NodeType::Looper).unwrap();

        graph.set_node_position(input_id, 0.0, 100.0);
        graph.set_node_position(master_mixer_id, 300.0, 100.0);
        graph.set_node_position(output_id, 500.0, 100.0);
        graph.set_node_position(metronome_id, 100.0, 250.0);
        graph.set_node_position(looper_id, 100.0, 350.0);

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: master_mixer_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: looper_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: metronome_id,
                source_port: PortId(0),
                target_node: master_mixer_id,
                target_port: PortId(1),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: looper_id,
                source_port: PortId(0),
                target_node: master_mixer_id,
                target_port: PortId(2),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: master_mixer_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph.commit_topology().unwrap();

        let (cmd_tx, cmd_rx): (crossbeam_channel::Sender<GraphCommand>, _) =
            crossbeam_channel::unbounded();

        Ok(Self {
            sample_rate,
            buffer_size,
            graph: Arc::new(ArcSwap::from_pointee(graph)),
            master_volume: Arc::new(Mutex::new(0.8)),
            input_gain: Arc::new(Mutex::new(1.0)),
            output_level: Arc::new(Mutex::new((0.0, 0.0))),
            input_level: Arc::new(Mutex::new((0.0, 0.0))),
            cpu_usage: Arc::new(Mutex::new(0.0)),
            dropout_count: Arc::new(Mutex::new(0)),
            input_channel: 0.min(in_channels.saturating_sub(1)),
            output_channels: (0, if out_channels > 1 { 1 } else { 0 }),
            stream: None,
            input_stream: None,
            host_id: Some(host_id),
            input_device_name: host
                .default_input_device()
                .map(|d| d.name().unwrap_or_default()),
            output_device_name: Some(output_device.name().unwrap_or_default()),
            command_tx: cmd_tx,
            command_rx: cmd_rx,
            chain_node_ids: Vec::new(),
            input_node_id: input_id,
            output_node_id: output_id,
            master_mixer_id,
            metronome_node_id: Some(metronome_id),
            looper_node_id: Some(looper_id),
            backing_track_node_id: None,
        })
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }

        let host = self.get_host()?;

        let output_device = match &self.output_device_name {
            Some(name) => Self::find_output_device(&host, name)?,
            None => host
                .default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No output device"))?,
        };

        let output_range = find_f32_output_config_range(&output_device, self.sample_rate as u32)
            .ok_or_else(|| anyhow::anyhow!("No suitable f32 output config"))?;

        let out_config = config_from_range(output_range, self.sample_rate as u32, self.buffer_size);
        let actual_sample_rate = out_config.sample_rate.0 as f64;
        let actual_buffer_size = match out_config.buffer_size {
            BufferSize::Fixed(bs) => bs,
            BufferSize::Default => self.buffer_size,
        };
        let out_ch_count = out_config.channels as usize;
        if out_ch_count == 0 {
            return Err(anyhow::anyhow!("Output device has 0 channels"));
        }

        self.sample_rate = actual_sample_rate;
        self.buffer_size = actual_buffer_size;
        {
            let guard = self.graph.load();
            let staging = (**guard).clone();
            drop(guard);
            commit_and_publish_graph(
                &self.graph,
                staging,
                Some((actual_sample_rate, actual_buffer_size as usize)),
            )?;
        }

        let cmd_rx = self.command_rx.clone();

        let graph = self.graph.clone();
        let master_volume = self.master_volume.clone();
        let input_gain = self.input_gain.clone();
        let output_level = self.output_level.clone();
        let input_level = self.input_level.clone();
        let cpu_usage = self.cpu_usage.clone();
        let dropout_count = self.dropout_count.clone();
        let output_ch = self.output_channels;

        let input_buffer = Arc::new(Mutex::new(InputFifo::new(
            2,
            (actual_buffer_size as usize).max(64) * 32,
            (actual_buffer_size as usize).max(64) * 2,
        )));
        let input_buffer_for_output = input_buffer.clone();
        let mut effective_input_ch = self.input_channel;

        let input_device = self
            .input_device_name
            .as_ref()
            .and_then(|name| Self::find_input_device(&host, name));

        if let Some(in_dev) = input_device {
            if let Some(in_range) =
                find_f32_input_config_range_exact(&in_dev, actual_sample_rate as u32)
            {
                let in_cfg =
                    config_from_range(in_range, actual_sample_rate as u32, actual_buffer_size);
                let in_ch_count = in_cfg.channels as usize;
                if in_ch_count > 0 {
                    effective_input_ch = self.input_channel.min(in_ch_count.saturating_sub(1));
                    self.input_channel = effective_input_ch;
                    let buf = input_buffer.clone();

                    let in_stream = in_dev.build_input_stream(
                        &in_cfg,
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            buf.lock().push_interleaved(data, in_ch_count);
                        },
                        |err| log::error!("Input stream error: {}", err),
                        None,
                    );

                    if let Ok(stream) = in_stream {
                        if stream.play().is_ok() {
                            self.input_stream = Some(stream);
                        }
                    }
                } else {
                    log::warn!("Input stream has 0 channels, skipping");
                }
            } else {
                log::warn!(
                    "Input device does not support {} Hz f32; input stream disabled to avoid rate mismatch",
                    actual_sample_rate as u32
                );
            }
        }

        let input_ch = effective_input_ch;

        let out_stream = output_device.build_output_stream(
            &out_config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let callback_start = std::time::Instant::now();
                let channels = out_ch_count.max(1);
                let num_frames = data.len() / channels;

                let gain = *input_gain.lock();

                let mut input_mono = vec![0.0f32; num_frames];
                {
                    input_buffer_for_output
                        .lock()
                        .pop_mono_into(input_ch, &mut input_mono, gain);
                }

                {
                    let mut il = input_level.lock();
                    let mut peak = 0.0f32;
                    for frame in 0..num_frames {
                        peak = peak.max(input_mono[frame].abs());
                    }
                    *il = (peak, peak);
                }

                {
                    let mut pending = Vec::new();
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        pending.push(cmd);
                    }
                    if !pending.is_empty() {
                        let guard = graph.load();
                        let mut staging = (**guard).clone();
                        drop(guard);
                        for cmd in pending {
                            apply_command(&mut staging, cmd);
                        }
                        if let Err(e) = commit_and_publish_graph(&graph, staging, None) {
                            log::error!("Topology commit in audio thread failed: {}", e);
                        }
                    }
                }

                let mut output_stereo = vec![vec![0.0f32; num_frames]; 2];
                {
                    let guard = graph.load();
                    let input = vec![input_mono];
                    output_stereo = guard.process(&input, num_frames);
                }

                let vol = *master_volume.lock();
                let out_l = output_ch.0.min(channels - 1);
                let out_r = output_ch.1.min(channels - 1);

                let mut peak_l = 0.0f32;
                let mut peak_r = 0.0f32;

                for frame in 0..num_frames {
                    let l = output_stereo[0].get(frame).copied().unwrap_or(0.0) * vol;
                    let r = output_stereo[1].get(frame).copied().unwrap_or(0.0) * vol;

                    peak_l = peak_l.max(l.abs());
                    peak_r = peak_r.max(r.abs());

                    data[frame * channels + out_l] = l;
                    if out_r != out_l {
                        data[frame * channels + out_r] = r;
                    }

                    for ch in 0..channels {
                        if ch != out_l && ch != out_r {
                            data[frame * channels + ch] = (l + r) * 0.5;
                        }
                    }
                }

                {
                    let mut ol = output_level.lock();
                    *ol = (peak_l, peak_r);
                }

                {
                    let elapsed = callback_start.elapsed().as_secs_f64();
                    let buffer_duration = num_frames as f64 / actual_sample_rate as f64;
                    let usage = (elapsed / buffer_duration * 100.0).min(100.0) as f32;
                    *cpu_usage.lock() = usage;
                }

                {
                    let ts = info.timestamp();
                    let delta = ts
                        .playback
                        .duration_since(&ts.callback)
                        .unwrap_or(std::time::Duration::ZERO);
                    let buffer_ns =
                        (num_frames as f64 / actual_sample_rate as f64) * 1_000_000_000.0;
                    if delta.as_nanos() as f64 > buffer_ns * 0.95 {
                        *dropout_count.lock() += 1;
                    }
                }
            },
            |err| log::error!("Output stream error: {}", err),
            None,
        );

        let out_stream =
            out_stream.map_err(|e| anyhow::anyhow!("Failed to build output stream: {}", e))?;
        out_stream
            .play()
            .map_err(|e| anyhow::anyhow!("Failed to play stream: {}", e))?;

        self.stream = Some(out_stream);

        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
        self.input_stream = None;
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    pub fn current_host_id(&self) -> Option<HostId> {
        self.host_id
    }

    pub(crate) fn get_host(&self) -> anyhow::Result<cpal::Host> {
        if let Some(target_id) = self.host_id {
            for hid in cpal::available_hosts() {
                if hid == target_id {
                    if let Ok(host) = cpal::host_from_id(hid) {
                        return Ok(host);
                    }
                }
            }
        }
        Ok(cpal::default_host())
    }

    pub(crate) fn find_output_device(
        host: &cpal::Host,
        name: &str,
    ) -> anyhow::Result<cpal::Device> {
        host.output_devices()
            .map_err(|e| anyhow::anyhow!("Cannot enumerate output devices: {}", e))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("Output device '{}' not found", name))
    }

    pub(crate) fn find_input_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
        host.input_devices()
            .ok()
            .and_then(|mut devs| devs.find(|d| d.name().map(|n| n == name).unwrap_or(false)))
    }
}
