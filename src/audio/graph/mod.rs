mod process;
mod processors;
mod processors_special;
mod state;
mod topology;

#[cfg(test)]
mod tests;

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::vst_host::plugin::LoadedPlugin;

use super::node::{ChannelConfig, Connection, NodeId, NodeInternalState, NodeType};

#[derive(Clone)]
pub struct SharedBuffer(Arc<Vec<Vec<f32>>>);

impl SharedBuffer {
    #[allow(dead_code)]
    pub fn new(channels: usize, frames: usize) -> Self {
        Self(Arc::new(vec![vec![0.0f32; frames]; channels]))
    }

    pub fn from_vec(data: Vec<Vec<f32>>) -> Self {
        Self(Arc::new(data))
    }

    pub fn as_slice(&self) -> &[Vec<f32>] {
        &self.0
    }

    #[allow(dead_code)]
    pub fn into_inner(self) -> Option<Vec<Vec<f32>>> {
        Arc::try_unwrap(self.0).ok()
    }
}

impl std::ops::Deref for SharedBuffer {
    type Target = [Vec<f32>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub enum GraphError {
    CycleDetected,
    ChannelMismatch {
        source: ChannelConfig,
        target: ChannelConfig,
    },
    NotFound,
    AlreadyConnected,
    SingletonViolation,
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::CycleDetected => write!(f, "Cycle detected in graph"),
            GraphError::ChannelMismatch { source, target } => {
                write!(f, "Channel mismatch: {:?} vs {:?}", source, target)
            }
            GraphError::NotFound => write!(f, "Node or port not found"),
            GraphError::AlreadyConnected => write!(f, "Target port already connected"),
            GraphError::SingletonViolation => {
                write!(f, "Only one instance of this node type is allowed")
            }
        }
    }
}

impl std::error::Error for GraphError {}

pub struct BackingTrackBuffer {
    pub data: Vec<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: f64,
    pub playback_pos: f64,
    pub total_frames: usize,
}

impl BackingTrackBuffer {
    pub fn new(data: Vec<Vec<f32>>, sample_rate: f64) -> Self {
        let channels = data.len();
        let total_frames = data.first().map(|c| c.len()).unwrap_or(0);
        Self {
            data,
            channels,
            sample_rate,
            playback_pos: 0.0,
            total_frames,
        }
    }

    pub fn clone_empty(&self) -> Self {
        Self {
            data: Vec::new(),
            channels: self.channels,
            sample_rate: self.sample_rate,
            playback_pos: 0.0,
            total_frames: 0,
        }
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate > 0.0 {
            self.total_frames as f64 / self.sample_rate
        } else {
            0.0
        }
    }
}

pub(super) struct LooperBuffer {
    pub data: Vec<Vec<f32>>,
    pub channels: usize,
    pub capacity: usize,
    pub write_pos: usize,
    pub len: usize,
    pub playback_pos: usize,
}

impl LooperBuffer {
    pub fn new(channels: usize, sample_rate: f64, max_seconds: f64) -> Self {
        let capacity = (sample_rate * max_seconds) as usize;
        let data = vec![vec![0.0f32; capacity]; channels];
        Self {
            data,
            channels,
            capacity,
            write_pos: 0,
            len: 0,
            playback_pos: 0,
        }
    }

    pub fn record(&mut self, input: &[Vec<f32>], num_frames: usize) {
        if self.capacity == 0 {
            return;
        }
        for frame in 0..num_frames {
            for ch in 0..self.channels.min(input.len()) {
                if self.write_pos < self.capacity {
                    self.data[ch][self.write_pos] = input[ch].get(frame).copied().unwrap_or(0.0);
                }
            }
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.len < self.capacity {
                self.len += 1;
            }
        }
    }

    pub fn overdub(&mut self, input: &[Vec<f32>], num_frames: usize) {
        if self.len == 0 {
            return;
        }
        for frame in 0..num_frames {
            let p = (self.playback_pos + frame) % self.len;
            for ch in 0..self.channels.min(input.len()) {
                let inp = input[ch].get(frame).copied().unwrap_or(0.0);
                self.data[ch][p] += inp;
            }
        }
    }

    pub fn read_and_advance(&mut self, output: &mut [Vec<f32>], num_frames: usize) {
        if self.len == 0 {
            return;
        }
        for frame in 0..num_frames {
            let p = self.playback_pos % self.len;
            for ch in 0..self.channels.min(output.len()) {
                if frame < output[ch].len() {
                    output[ch][frame] += self.data[ch][p];
                }
            }
            self.playback_pos = (self.playback_pos + 1) % self.len;
        }
    }

    pub fn clear(&mut self) {
        for ch in &mut self.data {
            ch.fill(0.0);
        }
        self.write_pos = 0;
        self.len = 0;
        self.playback_pos = 0;
    }

    pub fn clone_empty(&self) -> Self {
        Self::new(self.channels, 48000.0, 120.0)
    }

    #[allow(dead_code)]
    pub fn export_wav_samples(&self) -> Option<Vec<Vec<f32>>> {
        if self.len == 0 {
            return None;
        }
        let mut result = vec![vec![0.0f32; self.len]; self.channels];
        for ch in 0..self.channels {
            result[ch][..self.len].copy_from_slice(&self.data[ch][..self.len]);
        }
        Some(result)
    }

    #[allow(dead_code)]
    pub fn import_samples(&mut self, samples: &[Vec<f32>], sample_count: usize) {
        self.clear();
        let count = sample_count.min(self.capacity);
        for ch in 0..self.channels.min(samples.len()) {
            let copy_len = count.min(samples[ch].len());
            self.data[ch][..copy_len].copy_from_slice(&samples[ch][..copy_len]);
        }
        self.len = count;
        self.write_pos = count % self.capacity;
    }
}

pub struct GraphNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub input_ports: Vec<super::node::Port>,
    pub output_ports: Vec<super::node::Port>,
    pub enabled: bool,
    pub bypassed: bool,
    pub position: (f32, f32),

    pub input_buffers: Mutex<Vec<Option<Vec<Vec<f32>>>>>,
    pub output_buffers: Mutex<Vec<Vec<Vec<f32>>>>,
    pub shared_outputs: Mutex<Vec<Option<SharedBuffer>>>,

    pub plugin_instance: Mutex<Option<LoadedPlugin>>,
    pub internal_state: NodeInternalState,

    pub(super) looper_buffer: Mutex<Option<Vec<LooperBuffer>>>,
    pub(super) backing_track_buffer: Mutex<Option<BackingTrackBuffer>>,
    pub(super) recorder_buffer: Mutex<Option<Vec<Vec<f32>>>>,
    pub(super) metronome_phase: Mutex<f64>,
    pub(super) metronome_click_remaining: Mutex<usize>,
    pub(super) backing_pre_roll_remaining: Mutex<usize>,
    pub(super) drum_phase: Mutex<f64>,
    pub(super) drum_step: Mutex<u8>,
}

impl Clone for GraphNode {
    fn clone(&self) -> Self {
        let looper_clone = if let Some(ref bufs) = *self.looper_buffer.lock() {
            Some(bufs.iter().map(|b| b.clone_empty()).collect::<Vec<_>>())
        } else {
            None
        };
        let bt_clone = if let Some(ref buf) = *self.backing_track_buffer.lock() {
            Some(buf.clone_empty())
        } else {
            None
        };
        Self {
            id: self.id,
            node_type: self.node_type.clone(),
            input_ports: self.input_ports.clone(),
            output_ports: self.output_ports.clone(),
            enabled: self.enabled,
            bypassed: self.bypassed,
            position: self.position,
            input_buffers: Mutex::new(self.input_buffers.lock().clone()),
            output_buffers: Mutex::new(self.output_buffers.lock().clone()),
            shared_outputs: Mutex::new(vec![None; self.output_ports.len()]),
            plugin_instance: Mutex::new(None),
            internal_state: self.internal_state.clone(),
            looper_buffer: Mutex::new(looper_clone),
            backing_track_buffer: Mutex::new(bt_clone),
            recorder_buffer: Mutex::new(None),
            metronome_phase: Mutex::new(*self.metronome_phase.lock()),
            metronome_click_remaining: Mutex::new(*self.metronome_click_remaining.lock()),
            backing_pre_roll_remaining: Mutex::new(0),
            drum_phase: Mutex::new(0.0),
            drum_step: Mutex::new(0),
        }
    }
}

impl GraphNode {
    pub fn new(id: NodeId, node_type: NodeType, max_frames: usize) -> Self {
        let input_ports = node_type.input_ports();
        let output_ports = node_type.output_ports();

        let output_buffers: Vec<Vec<Vec<f32>>> = output_ports
            .iter()
            .map(|p| vec![vec![0.0f32; max_frames]; p.channels.channel_count()])
            .collect();

        let input_buffers: Vec<Option<Vec<Vec<f32>>>> = input_ports.iter().map(|_| None).collect();

        let internal_state = match &node_type {
            NodeType::Metronome => NodeInternalState::Metronome(super::node::MetronomeNodeState {
                bpm: 120.0,
                volume: 0.5,
                count_in_beats: 0,
                count_in_active: false,
            }),
            NodeType::Looper => NodeInternalState::Looper(super::node::LooperNodeState {
                enabled: false,
                recording: false,
                playing: false,
                overdubbing: false,
                cleared: false,
                fixed_length_beats: None,
                quantize_start: false,
                pre_fader: false,
                active_track: 0,
            }),
            NodeType::Gain => NodeInternalState::Gain { value: 1.0 },
            NodeType::Pan => NodeInternalState::Pan { value: 0.0 },
            NodeType::WetDry => NodeInternalState::WetDry { mix: 1.0 },
            NodeType::SendBus { .. } => NodeInternalState::SendBus { send_level: 1.0 },
            NodeType::DrumMachine => {
                NodeInternalState::DrumMachine(super::node::DrumMachineNodeState {
                    bpm: 120.0,
                    volume: 0.8,
                    playing: false,
                    pattern: 0,
                    current_step: 0,
                })
            }
            NodeType::Recorder => NodeInternalState::Recorder(super::node::RecorderNodeState {
                recording: false,
                has_data: false,
            }),
            NodeType::BackingTrack => {
                NodeInternalState::BackingTrack(super::node::BackingTrackNodeState {
                    playing: false,
                    volume: 1.0,
                    speed: 1.0,
                    looping: true,
                    file_loaded: false,
                    loop_start: None,
                    loop_end: None,
                    pitch_semitones: 0.0,
                    pre_roll_secs: 0.0,
                    section_markers: vec![],
                })
            }
            _ => NodeInternalState::None,
        };

        let looper_buffer = if matches!(node_type, NodeType::Looper) {
            let out_port = output_ports.first();
            let ch = out_port.map(|p| p.channels.channel_count()).unwrap_or(2);
            Mutex::new(Some(
                (0..4)
                    .map(|_| LooperBuffer::new(ch, 48000.0, 120.0))
                    .collect(),
            ))
        } else {
            Mutex::new(None)
        };

        let shared_count = output_ports.len();
        Self {
            id,
            node_type,
            input_ports,
            output_ports,
            enabled: true,
            bypassed: false,
            position: (0.0, 0.0),
            input_buffers: Mutex::new(input_buffers),
            output_buffers: Mutex::new(output_buffers),
            shared_outputs: Mutex::new(vec![None; shared_count]),
            plugin_instance: Mutex::new(None),
            internal_state,
            looper_buffer,
            backing_track_buffer: Mutex::new(None),
            recorder_buffer: Mutex::new(None),
            metronome_phase: Mutex::new(0.0),
            metronome_click_remaining: Mutex::new(0),
            backing_pre_roll_remaining: Mutex::new(0),
            drum_phase: Mutex::new(0.0),
            drum_step: Mutex::new(0),
        }
    }

    #[allow(dead_code)]
    pub fn resize_buffers(&self, max_frames: usize) {
        let mut output_buffers = self.output_buffers.lock();
        for (i, port) in self.output_ports.iter().enumerate() {
            let ch_count = port.channels.channel_count();
            if let Some(buf) = output_buffers.get_mut(i) {
                buf.resize(ch_count, vec![0.0f32; max_frames]);
                for ch in buf.iter_mut() {
                    ch.resize(max_frames, 0.0);
                }
            }
        }
        let mut input_buffers = self.input_buffers.lock();
        for opt_buf in input_buffers.iter_mut() {
            if let Some(buf) = opt_buf {
                for ch in buf.iter_mut() {
                    ch.resize(max_frames, 0.0);
                }
            }
        }
    }

    pub fn clear_output_buffers(&self) {
        {
            let mut output_buffers = self.output_buffers.lock();
            for port_buf in output_buffers.iter_mut() {
                for ch in port_buf.iter_mut() {
                    ch.fill(0.0);
                }
            }
        }
        {
            let mut shared = self.shared_outputs.lock();
            for s in shared.iter_mut() {
                *s = None;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CompiledConnection {
    pub source_node: NodeId,
    pub source_port_idx: usize,
    pub target_port_idx: usize,
}

pub struct AudioGraph {
    pub(super) nodes: HashMap<NodeId, GraphNode>,
    pub(super) connections: Vec<Connection>,
    pub(super) process_order: Vec<NodeId>,
    pub(super) compiled_connections: HashMap<NodeId, Vec<CompiledConnection>>,
    pub(super) input_node_id: Option<NodeId>,
    pub(super) output_node_id: Option<NodeId>,
    pub(super) max_frames: usize,
    pub(super) sample_rate: f64,
    pub(super) topology_dirty: bool,
    pub(super) next_node_id: u64,
}

impl Clone for AudioGraph {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.iter().map(|(&id, n)| (id, n.clone())).collect(),
            connections: self.connections.clone(),
            process_order: self.process_order.clone(),
            compiled_connections: self.compiled_connections.clone(),
            input_node_id: self.input_node_id,
            output_node_id: self.output_node_id,
            max_frames: self.max_frames,
            sample_rate: self.sample_rate,
            topology_dirty: self.topology_dirty,
            next_node_id: self.next_node_id,
        }
    }
}

impl AudioGraph {
    pub fn new(sample_rate: f64, max_frames: usize) -> Self {
        Self {
            nodes: HashMap::new(),
            connections: Vec::new(),
            process_order: Vec::new(),
            compiled_connections: HashMap::new(),
            input_node_id: None,
            output_node_id: None,
            max_frames,
            sample_rate,
            topology_dirty: true,
            next_node_id: 1,
        }
    }

    pub(super) fn allocate_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }
}
