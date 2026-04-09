use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, HostId, SampleFormat, Stream};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::audio::chain::Chain;
use crate::audio::graph::AudioGraph;
use crate::audio::graph_command::GraphCommand;
use crate::audio::node::{
    Connection, NodeId, NodeInternalState, NodeType, PortId, SerializedGraph,
};
use crate::looper::Looper;
use crate::metronome::Metronome;
use crate::vst_host::plugin::LoadedPlugin;
use crate::vst_host::scanner::PluginInfo;

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

struct InputFifo {
    channels: Vec<VecDeque<f32>>,
    max_frames: usize,
    target_frames: usize,
}

impl InputFifo {
    fn new(max_channels: usize, max_frames: usize, target_frames: usize) -> Self {
        Self {
            channels: (0..max_channels)
                .map(|_| VecDeque::with_capacity(max_frames))
                .collect(),
            max_frames,
            target_frames: target_frames.min(max_frames),
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        if self.channels.len() >= channels {
            return;
        }
        self.channels.extend(
            (self.channels.len()..channels).map(|_| VecDeque::with_capacity(self.max_frames)),
        );
    }

    fn available_frames(&self) -> usize {
        self.channels.iter().map(VecDeque::len).min().unwrap_or(0)
    }

    fn trim_to_capacity(&mut self) {
        let overflow = self.available_frames().saturating_sub(self.max_frames);
        if overflow == 0 {
            return;
        }

        for channel in &mut self.channels {
            for _ in 0..overflow.min(channel.len()) {
                let _ = channel.pop_front();
            }
        }
    }

    fn rebalance_for_output(&mut self, requested_frames: usize) {
        let available = self.available_frames();
        let keep = self.target_frames.saturating_add(requested_frames);
        let overflow = available.saturating_sub(keep);
        if overflow == 0 {
            return;
        }

        for channel in &mut self.channels {
            for _ in 0..overflow.min(channel.len()) {
                let _ = channel.pop_front();
            }
        }
    }

    fn push_interleaved(&mut self, data: &[f32], channels: usize) {
        if channels == 0 {
            return;
        }

        self.ensure_channels(channels);
        let frames = data.len() / channels;
        for frame in 0..frames {
            for ch in 0..channels {
                self.channels[ch].push_back(data[frame * channels + ch]);
            }
        }
        self.trim_to_capacity();
    }

    fn pop_mono_into(&mut self, channel: usize, output: &mut [f32], gain: f32) {
        output.fill(0.0);
        if self.channels.is_empty() {
            return;
        }

        self.rebalance_for_output(output.len());
        let frames = self.available_frames().min(output.len());
        if frames == 0 {
            return;
        }

        let src_ch = channel.min(self.channels.len().saturating_sub(1));
        for frame in 0..frames {
            if let Some(sample) = self.channels[src_ch].pop_front() {
                output[frame] = sample * gain;
            }
        }

        for (ch, queue) in self.channels.iter_mut().enumerate() {
            if ch == src_ch {
                continue;
            }
            for _ in 0..frames.min(queue.len()) {
                let _ = queue.pop_front();
            }
        }
    }
}

pub struct AudioEngine {
    pub sample_rate: f64,
    pub buffer_size: u32,
    pub chain: Arc<Mutex<Chain>>,
    pub graph: Arc<ArcSwap<AudioGraph>>,
    pub metronome: Arc<Mutex<Metronome>>,
    pub looper: Arc<Mutex<Looper>>,
    pub master_volume: Arc<Mutex<f32>>,
    pub input_gain: Arc<Mutex<f32>>,
    pub output_level: Arc<Mutex<(f32, f32)>>,
    pub input_level: Arc<Mutex<(f32, f32)>>,
    pub input_channel: usize,
    pub output_channels: (usize, usize),
    #[allow(dead_code)]
    pub chain_node_ids: Vec<NodeId>,
    #[allow(dead_code)]
    pub input_node_id: NodeId,
    #[allow(dead_code)]
    pub output_node_id: NodeId,

    pub metronome_node_id: Option<NodeId>,
    pub looper_node_id: Option<NodeId>,

    stream: Option<Stream>,
    input_stream: Option<Stream>,
    host_id: Option<HostId>,
    input_device_name: Option<String>,
    output_device_name: Option<String>,

    command_tx: crossbeam_channel::Sender<GraphCommand>,
    command_rx: crossbeam_channel::Receiver<GraphCommand>,
}

fn apply_command(graph: &mut AudioGraph, cmd: GraphCommand) {
    match cmd {
        GraphCommand::AddNode(node_type) => {
            if let Ok(_id) = graph.add_node(node_type) {
                log::debug!("GraphCommand::AddNode processed");
            }
        }
        GraphCommand::RemoveNode(id) => {
            graph.remove_node(id);
            log::debug!("GraphCommand::RemoveNode({:?}) processed", id);
        }
        GraphCommand::SetNodeEnabled(id, enabled) => {
            graph.set_node_enabled(id, enabled);
        }
        GraphCommand::SetNodeBypassed(id, bypassed) => {
            graph.set_node_bypassed(id, bypassed);
        }
        GraphCommand::SetNodeState(id, state) => {
            if let Some(node) = graph.get_node_mut(id) {
                if let NodeInternalState::Looper(ref looper_state) = state {
                    if looper_state.cleared {
                        graph.clear_looper(id);
                        return;
                    }
                }
                node.internal_state = state;
            }
        }
        GraphCommand::SetNodePosition(id, x, y) => {
            graph.set_node_position(id, x, y);
        }
        GraphCommand::Connect(conn) => {
            if let Err(e) = graph.connect(conn) {
                log::warn!("GraphCommand::Connect failed: {}", e);
            }
        }
        GraphCommand::Disconnect { source, target } => {
            graph.disconnect(source, target);
        }
        GraphCommand::CommitTopology => {
            if let Err(e) = graph.commit_topology() {
                log::error!("GraphCommand::CommitTopology failed: {}", e);
            }
        }
    }
}

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

fn find_f32_output_config_range(
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

fn find_f32_input_config_range(
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

fn find_f32_input_config_range_exact(
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

fn prepare_runtime_graph(staging: &mut AudioGraph, runtime_config: Option<(f64, usize)>) {
    let Some((sample_rate, max_frames)) = runtime_config else {
        return;
    };

    staging.set_sample_rate(sample_rate);
    staging.set_max_frames(max_frames);

    let looper_ids: Vec<NodeId> = staging
        .nodes()
        .iter()
        .filter_map(|(&id, node)| matches!(node.node_type, NodeType::Looper).then_some(id))
        .collect();

    for looper_id in looper_ids {
        staging.init_looper_buffer(looper_id);
    }
}

fn transfer_runtime_plugins(
    current: &AudioGraph,
    staging: &mut AudioGraph,
    runtime_config: Option<(f64, usize)>,
) -> anyhow::Result<()> {
    if let Some((sample_rate, max_frames)) = runtime_config {
        for node in current.nodes().values() {
            let mut plugin_instance = node.plugin_instance.lock();
            if let Some(ref mut plugin) = *plugin_instance {
                plugin.setup_processing(sample_rate, max_frames as i32)?;
            }
        }
    }

    for (&id, source_node) in current.nodes() {
        let Some(target_node) = staging.get_node(id) else {
            continue;
        };

        let maybe_plugin = source_node.plugin_instance.lock().take();
        if let Some(plugin) = maybe_plugin {
            *target_node.plugin_instance.lock() = Some(plugin);
        }
    }

    Ok(())
}

fn commit_and_publish_graph(
    graph: &Arc<ArcSwap<AudioGraph>>,
    mut staging: AudioGraph,
    runtime_config: Option<(f64, usize)>,
) -> anyhow::Result<()> {
    prepare_runtime_graph(&mut staging, runtime_config);
    staging
        .commit_topology()
        .map_err(|e| anyhow::anyhow!("Topology commit failed: {}", e))?;

    let guard = graph.load();
    transfer_runtime_plugins(&guard, &mut staging, runtime_config)?;
    drop(guard);
    graph.store(Arc::new(staging));
    Ok(())
}

fn apply_serialized_parameters(plugin: &mut LoadedPlugin, parameters: &[(u32, f32)]) {
    for (index, value) in parameters {
        plugin.set_parameter(*index as usize, *value);
    }
}

fn config_from_range(
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

        let num_channels = out_channels.max(in_channels).max(2);

        let mut graph = AudioGraph::new(sample_rate, buffer_size as usize);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();
        graph
            .connect(Connection {
                source_node: input_id,
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
            chain: Arc::new(Mutex::new(Chain::new())),
            graph: Arc::new(ArcSwap::from_pointee(graph)),
            metronome: Arc::new(Mutex::new(Metronome::new(sample_rate))),
            looper: Arc::new(Mutex::new(Looper::new(num_channels, sample_rate))),
            master_volume: Arc::new(Mutex::new(0.8)),
            input_gain: Arc::new(Mutex::new(1.0)),
            output_level: Arc::new(Mutex::new((0.0, 0.0))),
            input_level: Arc::new(Mutex::new((0.0, 0.0))),
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
            metronome_node_id: None,
            looper_node_id: None,
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
            let mut met = self.metronome.lock();
            met.set_sample_rate(actual_sample_rate);
        }
        {
            let mut lpr = self.looper.lock();
            lpr.set_config(out_ch_count.max(2), actual_sample_rate);
        }
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
        let metronome = self.metronome.clone();
        let looper = self.looper.clone();
        let master_volume = self.master_volume.clone();
        let input_gain = self.input_gain.clone();
        let output_level = self.output_level.clone();
        let input_level = self.input_level.clone();
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
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
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

                {
                    let mut met = metronome.lock();
                    let mut slices: Vec<&mut [f32]> =
                        output_stereo.iter_mut().map(|v| &mut v[..]).collect();
                    met.process(&mut slices, num_frames);
                }

                {
                    let mut lpr = looper.lock();
                    let mut slices: Vec<&mut [f32]> =
                        output_stereo.iter_mut().map(|v| &mut v[..]).collect();
                    lpr.process(&mut slices, num_frames);
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

    pub fn send_command(&self, cmd: GraphCommand) {
        if let Err(e) = self.command_tx.send(cmd) {
            log::error!("Failed to send graph command: {:?}", e);
        }
    }

    pub fn apply_commands_to_staging(&self) {
        let mut pending = Vec::new();
        while let Ok(cmd) = self.command_rx.try_recv() {
            pending.push(cmd);
        }
        if !pending.is_empty() {
            let guard = self.graph.load();
            let mut staging = (**guard).clone();
            drop(guard);
            for cmd in pending {
                apply_command(&mut staging, cmd);
            }
            if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
                log::error!("Topology commit in staging failed: {}", e);
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    pub fn current_host_id(&self) -> Option<HostId> {
        self.host_id
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

    fn get_host(&self) -> anyhow::Result<cpal::Host> {
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

    fn find_output_device(host: &cpal::Host, name: &str) -> anyhow::Result<cpal::Device> {
        host.output_devices()
            .map_err(|e| anyhow::anyhow!("Cannot enumerate output devices: {}", e))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("Output device '{}' not found", name))
    }

    fn find_input_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
        host.input_devices()
            .ok()
            .and_then(|mut devs| devs.find(|d| d.name().map(|n| n == name).unwrap_or(false)))
    }

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

    pub fn graph_add_node(&self, node_type: NodeType) {
        self.send_command(GraphCommand::AddNode(node_type));
    }

    pub fn graph_remove_node(&self, id: NodeId) {
        self.send_command(GraphCommand::RemoveNode(id));
    }

    pub fn graph_connect(&self, conn: Connection) {
        self.send_command(GraphCommand::Connect(conn));
    }

    pub fn graph_disconnect(&self, source: (NodeId, PortId), target: (NodeId, PortId)) {
        self.send_command(GraphCommand::Disconnect { source, target });
    }

    pub fn graph_set_enabled(&self, id: NodeId, enabled: bool) {
        self.send_command(GraphCommand::SetNodeEnabled(id, enabled));
    }

    pub fn graph_set_bypassed(&self, id: NodeId, bypassed: bool) {
        self.send_command(GraphCommand::SetNodeBypassed(id, bypassed));
    }

    pub fn graph_set_state(&self, id: NodeId, state: NodeInternalState) {
        self.send_command(GraphCommand::SetNodeState(id, state));
    }

    pub fn graph_commit_topology(&self) {
        self.send_command(GraphCommand::CommitTopology);
    }

    #[allow(dead_code)]
    pub fn graph_send_command(&self, cmd: GraphCommand) {
        self.send_command(cmd);
    }

    pub fn add_node_with_position(&self, node_type: NodeType, x: f32, y: f32) -> NodeId {
        self.graph_add_node(node_type);
        self.apply_commands_to_staging();
        let guard = self.graph.load();
        let max_id = guard.nodes().keys().max().copied().unwrap_or(NodeId(0));
        drop(guard);
        self.send_command(GraphCommand::SetNodePosition(max_id, x, y));
        self.apply_commands_to_staging();
        max_id
    }

    pub fn add_metronome_node(&mut self) -> NodeId {
        if let Some(id) = self.metronome_node_id {
            return id;
        }
        self.graph_add_node(NodeType::Metronome);
        self.apply_commands_to_staging();
        let id = self.find_node_id_by_type(NodeType::Metronome);
        if let Some(id) = id {
            self.metronome_node_id = Some(id);
            return id;
        }
        NodeId(0)
    }

    pub fn add_looper_node(&mut self) -> NodeId {
        if let Some(id) = self.looper_node_id {
            return id;
        }
        self.graph_add_node(NodeType::Looper);
        self.apply_commands_to_staging();
        let id = self.find_node_id_by_type(NodeType::Looper);
        if let Some(id) = id {
            {
                let guard = self.graph.load();
                let mut staging = (**guard).clone();
                drop(guard);
                staging.init_looper_buffer(id);
                if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
                    log::error!("Failed to initialize looper buffer: {}", e);
                    return NodeId(0);
                }
            }
            self.looper_node_id = Some(id);
            return id;
        }
        NodeId(0)
    }

    fn find_node_id_by_type(&self, target: NodeType) -> Option<NodeId> {
        let guard = self.graph.load();
        for (&id, node) in guard.nodes() {
            if std::mem::discriminant(&node.node_type) == std::mem::discriminant(&target) {
                return Some(id);
            }
        }
        None
    }

    pub fn snapshot_serialized_graph(&self) -> SerializedGraph {
        let guard = self.graph.load();
        let mut node_ids: Vec<NodeId> = guard.nodes().keys().copied().collect();
        node_ids.sort();

        let mut nodes = Vec::with_capacity(node_ids.len());
        for node_id in node_ids {
            let Some(node) = guard.get_node(node_id) else {
                continue;
            };

            let (parameters, plugin_state) = if matches!(node.node_type, NodeType::VstPlugin { .. })
            {
                let plugin_instance = node.plugin_instance.lock();
                if let Some(ref plugin) = *plugin_instance {
                    let parameters = plugin
                        .parameter_info()
                        .iter()
                        .enumerate()
                        .map(|(index, _)| (index as u32, plugin.get_parameter(index)))
                        .collect();
                    (parameters, plugin.save_state())
                } else {
                    (Vec::new(), None)
                }
            } else {
                (Vec::new(), None)
            };

            nodes.push(crate::audio::node::SerializedNode {
                id: node_id,
                node_type: node.node_type.clone(),
                enabled: node.enabled,
                bypassed: node.bypassed,
                position: node.position,
                parameters,
                plugin_state,
                internal_state: node.internal_state.clone(),
            });
        }

        let mut connections = guard.connections().to_vec();
        connections.sort_by_key(|conn| {
            (
                conn.source_node.0,
                conn.source_port.0,
                conn.target_node.0,
                conn.target_port.0,
            )
        });

        SerializedGraph { nodes, connections }
    }

    fn instantiate_serialized_plugin(
        &self,
        info: &PluginInfo,
        plugin_state: Option<&[u8]>,
        parameters: &[(u32, f32)],
    ) -> anyhow::Result<LoadedPlugin> {
        let mut plugin = LoadedPlugin::load(info)?;
        plugin.setup_processing(self.sample_rate, self.buffer_size as i32)?;

        let restored_from_state = if let Some(state) = plugin_state {
            match plugin.restore_state(state) {
                Ok(()) => true,
                Err(err) => {
                    log::warn!(
                        "Failed to restore saved state for '{}' from '{}': {}",
                        info.name,
                        info.path.display(),
                        err
                    );
                    false
                }
            }
        } else {
            false
        };

        if !restored_from_state {
            apply_serialized_parameters(&mut plugin, parameters);
        }

        Ok(plugin)
    }

    pub fn load_serialized_graph(&mut self, data: &SerializedGraph) -> anyhow::Result<()> {
        let mut new_graph = AudioGraph::new(self.sample_rate, self.buffer_size as usize);

        for sn in &data.nodes {
            new_graph
                .add_node_with_id(sn.id, sn.node_type.clone())
                .map_err(|err| anyhow::anyhow!("Failed to add node {:?}: {}", sn.id, err))?;

            new_graph.set_node_enabled(sn.id, sn.enabled);
            new_graph.set_node_bypassed(sn.id, sn.bypassed);
            new_graph.set_node_position(sn.id, sn.position.0, sn.position.1);
            if !matches!(sn.internal_state, NodeInternalState::None) {
                new_graph.set_node_internal_state(sn.id, sn.internal_state.clone());
            }
        }

        for conn in &data.connections {
            new_graph
                .connect(conn.clone())
                .map_err(|err| anyhow::anyhow!("Failed to connect {:?}: {}", conn, err))?;
        }

        prepare_runtime_graph(
            &mut new_graph,
            Some((self.sample_rate, self.buffer_size as usize)),
        );
        new_graph
            .commit_topology()
            .map_err(|err| anyhow::anyhow!("Topology commit failed: {}", err))?;

        for sn in &data.nodes {
            let NodeType::VstPlugin {
                plugin_path,
                plugin_name,
            } = &sn.node_type
            else {
                continue;
            };

            let info = PluginInfo {
                path: std::path::PathBuf::from(plugin_path),
                name: plugin_name.clone(),
                category: String::new(),
                vendor: String::new(),
            };

            match self.instantiate_serialized_plugin(
                &info,
                sn.plugin_state.as_deref(),
                &sn.parameters,
            ) {
                Ok(plugin) => {
                    if let Some(node) = new_graph.get_node(sn.id) {
                        *node.plugin_instance.lock() = Some(plugin);
                    }
                }
                Err(err) => {
                    log::error!(
                        "Failed to restore VST '{}' from '{}': {}",
                        plugin_name,
                        plugin_path,
                        err
                    );
                }
            }
        }

        let metronome_node_id = Self::find_node_id_in_graph(&new_graph, NodeType::Metronome);
        let looper_node_id = Self::find_node_id_in_graph(&new_graph, NodeType::Looper);
        self.metronome_node_id = metronome_node_id;
        self.looper_node_id = looper_node_id;
        self.graph.store(Arc::new(new_graph));
        log::info!(
            "Loaded serialized graph: {} nodes, {} connections",
            data.nodes.len(),
            data.connections.len()
        );
        Ok(())
    }

    pub fn load_vst_plugin_to_node(
        &self,
        node_id: NodeId,
        info: &PluginInfo,
    ) -> anyhow::Result<()> {
        let plugin = self.instantiate_serialized_plugin(info, None, &[])?;

        {
            let guard = self.graph.load();
            if let Some(node) = guard.get_node(node_id) {
                *node.plugin_instance.lock() = Some(plugin);
            } else {
                return Err(anyhow::anyhow!("Node {:?} not found in graph", node_id));
            }
        }

        log::info!("VST plugin '{}' loaded into node {:?}", info.name, node_id);
        Ok(())
    }

    fn find_node_id_in_graph(graph: &AudioGraph, target: NodeType) -> Option<NodeId> {
        for (&id, node) in graph.nodes() {
            if std::mem::discriminant(&node.node_type) == std::mem::discriminant(&target) {
                return Some(id);
            }
        }
        None
    }

    pub fn set_vst_node_parameter(&self, node_id: NodeId, param_index: usize, value: f32) {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node(node_id) {
            let mut plugin_instance = node.plugin_instance.lock();
            if let Some(ref mut plugin) = *plugin_instance {
                plugin.set_parameter(param_index, value);
            }
        }
    }

    pub fn get_vst_node_parameters(&self, node_id: NodeId) -> Vec<crate::audio::chain::ParamInfo> {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node(node_id) {
            let plugin_instance = node.plugin_instance.lock();
            if let Some(ref plugin) = *plugin_instance {
                return plugin.parameter_info();
            }
        }
        Vec::new()
    }

    pub fn get_vst_node_parameter_value(&self, node_id: NodeId, param_index: usize) -> f32 {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node(node_id) {
            let plugin_instance = node.plugin_instance.lock();
            if let Some(ref plugin) = *plugin_instance {
                return plugin.get_parameter(param_index);
            }
        }
        0.0
    }

    pub fn execute_undo_actions(&self, actions: &[crate::undo::UndoAction]) {
        let guard = self.graph.load();
        let mut staging = (**guard).clone();
        drop(guard);

        for action in actions {
            match action {
                crate::undo::UndoAction::AddedNode { node_id, .. } => {
                    staging.remove_node(*node_id);
                }
                crate::undo::UndoAction::RemovedNode {
                    node_id,
                    node_type,
                    position,
                    enabled,
                    bypassed,
                    state,
                    connections,
                } => {
                    let _ = staging.add_node_with_id(*node_id, node_type.clone());
                    staging.set_node_position(*node_id, position.0, position.1);
                    staging.set_node_enabled(*node_id, *enabled);
                    staging.set_node_bypassed(*node_id, *bypassed);
                    staging.set_node_internal_state(*node_id, state.clone());
                    for conn in connections {
                        let _ = staging.connect(conn.clone());
                    }
                }
                crate::undo::UndoAction::Connected(conn) => {
                    staging.disconnect(
                        (conn.source_node, conn.source_port),
                        (conn.target_node, conn.target_port),
                    );
                }
                crate::undo::UndoAction::Disconnected(conn) => {
                    let _ = staging.connect(conn.clone());
                }
                crate::undo::UndoAction::MovedNode {
                    node_id, old_pos, ..
                } => {
                    staging.set_node_position(*node_id, old_pos.0, old_pos.1);
                }
                crate::undo::UndoAction::ChangedState {
                    node_id, old_state, ..
                } => {
                    staging.set_node_internal_state(*node_id, old_state.clone());
                }
                crate::undo::UndoAction::ChangedBypass {
                    node_id,
                    old_bypassed,
                    ..
                } => {
                    staging.set_node_bypassed(*node_id, *old_bypassed);
                }
            }
        }

        if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
            log::error!("Undo commit_topology failed: {}", e);
        }
    }

    pub fn execute_redo_actions(&self, actions: &[crate::undo::UndoAction]) {
        let guard = self.graph.load();
        let mut staging = (**guard).clone();
        drop(guard);

        for action in actions {
            match action {
                crate::undo::UndoAction::AddedNode {
                    node_id,
                    node_type,
                    position,
                } => {
                    let _ = staging.add_node_with_id(*node_id, node_type.clone());
                    staging.set_node_position(*node_id, position.0, position.1);
                }
                crate::undo::UndoAction::RemovedNode { node_id, .. } => {
                    staging.remove_node(*node_id);
                }
                crate::undo::UndoAction::Connected(conn) => {
                    let _ = staging.connect(conn.clone());
                }
                crate::undo::UndoAction::Disconnected(conn) => {
                    staging.disconnect(
                        (conn.source_node, conn.source_port),
                        (conn.target_node, conn.target_port),
                    );
                }
                crate::undo::UndoAction::MovedNode {
                    node_id, new_pos, ..
                } => {
                    staging.set_node_position(*node_id, new_pos.0, new_pos.1);
                }
                crate::undo::UndoAction::ChangedState {
                    node_id, new_state, ..
                } => {
                    staging.set_node_internal_state(*node_id, new_state.clone());
                }
                crate::undo::UndoAction::ChangedBypass {
                    node_id,
                    new_bypassed,
                    ..
                } => {
                    staging.set_node_bypassed(*node_id, *new_bypassed);
                }
            }
        }

        if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
            log::error!("Redo commit_topology failed: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InputFifo;

    #[test]
    fn input_fifo_reblocks_input_for_output() {
        let mut fifo = InputFifo::new(2, 32, 4);

        fifo.push_interleaved(&[1.0, 10.0, 2.0, 20.0], 2);
        fifo.push_interleaved(&[3.0, 30.0, 4.0, 40.0], 2);

        let mut output = vec![0.0f32; 4];
        fifo.pop_mono_into(0, &mut output, 1.0);

        assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn input_fifo_keeps_channels_aligned_when_trimming() {
        let mut fifo = InputFifo::new(2, 3, 1);
        fifo.push_interleaved(&[1.0, 10.0, 2.0, 20.0, 3.0, 30.0, 4.0, 40.0], 2);

        let mut output = vec![0.0f32; 3];
        fifo.pop_mono_into(1, &mut output, 1.0);

        assert_eq!(output, vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn input_fifo_rebalances_latency_before_output() {
        let mut fifo = InputFifo::new(1, 16, 2);
        fifo.push_interleaved(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 1);

        let mut output = vec![0.0f32; 2];
        fifo.pop_mono_into(0, &mut output, 1.0);

        assert_eq!(output, vec![3.0, 4.0]);
    }
}
