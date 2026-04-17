mod process;
mod processors;
mod processors_special;
mod state;
mod topology;

#[cfg(test)]
mod tests;

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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

#[derive(Clone)]
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

#[derive(Clone)]
pub(crate) struct LooperBuffer {
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
        let ch_count = self.channels.min(input.len());
        let avail = self.capacity - self.write_pos;
        let first = num_frames.min(avail);
        for ch in 0..ch_count {
            let src = &input[ch];
            let dst = &mut self.data[ch];
            let copy_len = first.min(src.len());
            dst[self.write_pos..self.write_pos + copy_len].copy_from_slice(&src[..copy_len]);
        }
        if first < num_frames {
            let second = num_frames - first;
            for ch in 0..ch_count {
                let src = &input[ch];
                let dst = &mut self.data[ch];
                let copy_len = second.min(src.len().saturating_sub(first));
                if copy_len > 0 {
                    dst[..copy_len].copy_from_slice(&src[first..first + copy_len]);
                }
            }
            self.write_pos = second;
        } else {
            self.write_pos += first;
        }
        self.len = (self.len + num_frames).min(self.capacity);
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
        let ch_count = self.channels.min(output.len());
        let avail = self.len - self.playback_pos;
        let first = num_frames.min(avail);
        for ch in 0..ch_count {
            let src = &self.data[ch];
            let dst = &mut output[ch];
            let copy_len = first.min(dst.len());
            for i in 0..copy_len {
                dst[i] += src[self.playback_pos + i];
            }
        }
        if first < num_frames {
            let second = num_frames - first;
            for ch in 0..ch_count {
                let src = &self.data[ch];
                let dst = &mut output[ch];
                let copy_len = second.min(dst.len().saturating_sub(first));
                for i in 0..copy_len {
                    dst[first + i] += src[i];
                }
            }
            self.playback_pos = second;
        } else {
            self.playback_pos += first;
        }
        if self.playback_pos >= self.len {
            self.playback_pos = 0;
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
        Self::new(
            self.channels,
            48000.0_f64.max(self.capacity as f64 / 120.0),
            120.0,
        )
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

pub(crate) struct ProcessBuffers {
    pub input_buffers: Vec<Option<Vec<Vec<f32>>>>,
    pub output_buffers: Vec<Vec<Vec<f32>>>,
    pub shared_outputs: Vec<Option<SharedBuffer>>,
    pub looper_buffer: Option<Vec<LooperBuffer>>,
    pub backing_track_buffer: Option<BackingTrackBuffer>,
    pub recorder_buffer: Option<Vec<Vec<f32>>>,
    pub metronome_phase: f64,
    pub metronome_click_remaining: usize,
    pub backing_pre_roll_remaining: Option<usize>,
    pub drum_phase: f64,
    pub drum_step: u8,
}

pub struct GraphNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub input_ports: Vec<super::node::Port>,
    pub output_ports: Vec<super::node::Port>,
    pub enabled: bool,
    pub bypassed: bool,
    pub position: (f32, f32),

    pub buffers: UnsafeCell<ProcessBuffers>,

    pub plugin_instance: parking_lot::Mutex<Option<LoadedPlugin>>,
    pub internal_state: NodeInternalState,

    pub atomic_bt_position: AtomicU64,
    pub atomic_bt_duration: AtomicU64,
    pub atomic_looper_lengths: [AtomicUsize; 4],
    pub atomic_recorder_has_data: AtomicBool,
}

// Safety: UnsafeCell<ProcessBuffers> is safe for audio-thread-only mutation via buffers_mut().
// UI-readable values (BT position/duration, looper lengths, recorder status) are exposed through
// atomic fields (atomic_bt_position, atomic_bt_duration, atomic_looper_lengths, atomic_recorder_has_data)
// which the audio thread updates after each process() call. The UI thread reads these atomics
// instead of accessing ProcessBuffers directly.
//
// Graph-change migration: when the UI publishes a new graph via ArcSwap, the audio thread detects
// the change on its next callback and calls AudioGraph::migrate_runtime_state_from(prev) to move
// plugin instances and heavy buffers forward. This runs single-threaded on the audio side, so the
// UnsafeCell is still only written by one thread at a time.
unsafe impl Sync for GraphNode {}
unsafe impl Send for GraphNode {}

impl Clone for GraphNode {
    fn clone(&self) -> Self {
        let b = self.buffers();
        let looper_clone = b
            .looper_buffer
            .as_ref()
            .map(|bufs| bufs.iter().map(|b| b.clone_empty()).collect());
        let bt_clone = b.backing_track_buffer.as_ref().map(|b| b.clone_empty());
        let new_buffers = ProcessBuffers {
            input_buffers: b.input_buffers.clone(),
            output_buffers: b.output_buffers.clone(),
            shared_outputs: vec![None; self.output_ports.len()],
            looper_buffer: looper_clone,
            backing_track_buffer: bt_clone,
            recorder_buffer: None,
            metronome_phase: b.metronome_phase,
            metronome_click_remaining: b.metronome_click_remaining,
            backing_pre_roll_remaining: None,
            drum_phase: 0.0,
            drum_step: 0,
        };
        Self {
            id: self.id,
            node_type: self.node_type.clone(),
            input_ports: self.input_ports.clone(),
            output_ports: self.output_ports.clone(),
            enabled: self.enabled,
            bypassed: self.bypassed,
            position: self.position,
            buffers: UnsafeCell::new(new_buffers),
            plugin_instance: parking_lot::Mutex::new(None),
            internal_state: self.internal_state.clone(),
            atomic_bt_position: AtomicU64::new(self.atomic_bt_position.load(Ordering::Relaxed)),
            atomic_bt_duration: AtomicU64::new(self.atomic_bt_duration.load(Ordering::Relaxed)),
            atomic_looper_lengths: std::array::from_fn(|i| {
                AtomicUsize::new(self.atomic_looper_lengths[i].load(Ordering::Relaxed))
            }),
            atomic_recorder_has_data: AtomicBool::new(
                self.atomic_recorder_has_data.load(Ordering::Relaxed),
            ),
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
            Some(
                (0..4)
                    .map(|_| LooperBuffer::new(ch, 48000.0, 120.0))
                    .collect(),
            )
        } else {
            None
        };

        let shared_count = output_ports.len();
        let buffers = ProcessBuffers {
            input_buffers,
            output_buffers,
            shared_outputs: vec![None; shared_count],
            looper_buffer,
            backing_track_buffer: None,
            recorder_buffer: None,
            metronome_phase: 0.0,
            metronome_click_remaining: 0,
            backing_pre_roll_remaining: None,
            drum_phase: 0.0,
            drum_step: 0,
        };
        Self {
            id,
            node_type,
            input_ports,
            output_ports,
            enabled: true,
            bypassed: false,
            position: (0.0, 0.0),
            buffers: UnsafeCell::new(buffers),
            plugin_instance: parking_lot::Mutex::new(None),
            internal_state,
            atomic_bt_position: AtomicU64::new(0.0f64.to_bits()),
            atomic_bt_duration: AtomicU64::new(0.0f64.to_bits()),
            atomic_looper_lengths: std::array::from_fn(|_| AtomicUsize::new(0)),
            atomic_recorder_has_data: AtomicBool::new(false),
        }
    }

    #[inline]
    pub fn buffers(&self) -> &ProcessBuffers {
        unsafe { &*self.buffers.get() }
    }

    #[inline]
    pub fn buffers_mut(&self) -> &mut ProcessBuffers {
        unsafe { &mut *self.buffers.get() }
    }

    #[allow(dead_code)]
    pub fn resize_buffers(&self, max_frames: usize) {
        let b = self.buffers_mut();
        for (i, port) in self.output_ports.iter().enumerate() {
            let ch_count = port.channels.channel_count();
            if let Some(buf) = b.output_buffers.get_mut(i) {
                buf.resize(ch_count, vec![0.0f32; max_frames]);
                for ch in buf.iter_mut() {
                    ch.resize(max_frames, 0.0);
                }
            }
        }
        for opt_buf in b.input_buffers.iter_mut() {
            if let Some(buf) = opt_buf {
                for ch in buf.iter_mut() {
                    ch.resize(max_frames, 0.0);
                }
            }
        }
    }

    pub fn clear_output_buffers(&self) {
        let b = self.buffers_mut();
        for port_buf in b.output_buffers.iter_mut() {
            for ch in port_buf.iter_mut() {
                ch.fill(0.0);
            }
        }
        for s in b.shared_outputs.iter_mut() {
            *s = None;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CompiledConnection {
    pub source_node: NodeId,
    pub source_port_idx: usize,
    pub target_port_idx: usize,
}

#[derive(Clone, Debug)]
pub(super) struct CompiledConnectionIdx {
    pub source_idx: usize,
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

    pub(super) nodes_vec: Vec<GraphNode>,
    pub(super) compiled_connections_vec: Vec<Vec<CompiledConnectionIdx>>,
    pub(super) id_to_index: HashMap<NodeId, usize>,
    pub(super) input_node_idx: Option<usize>,
    pub(super) output_node_idx: Option<usize>,
    pub(super) metronome_idx: Option<usize>,

    /// When set, the audio thread skips the first runtime-state migration on
    /// the callback that picks this graph up. Used by preset loads where the
    /// new graph should start with fresh looper/backing-track/recorder state
    /// rather than inheriting the previously-live graph's buffers. Consumed
    /// (cleared) by the audio thread after one migration check.
    pub(super) skip_runtime_migration: std::sync::atomic::AtomicBool,
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
            // Force topology rebuild after clone: the runtime side-tables below
            // (nodes_vec, compiled_connections_vec, id_to_index, *_node_idx) are
            // intentionally not cloned, so commit_topology() MUST run before the
            // clone is published or the audio thread sees an empty graph.
            topology_dirty: true,
            next_node_id: self.next_node_id,
            nodes_vec: Vec::new(),
            compiled_connections_vec: Vec::new(),
            id_to_index: HashMap::new(),
            input_node_idx: None,
            output_node_idx: None,
            metronome_idx: None,
            // Default: migration is allowed. Preset load paths set this to
            // true on the staging graph before publishing.
            skip_runtime_migration: std::sync::atomic::AtomicBool::new(false),
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
            nodes_vec: Vec::new(),
            compiled_connections_vec: Vec::new(),
            id_to_index: HashMap::new(),
            input_node_idx: None,
            output_node_idx: None,
            metronome_idx: None,
            skip_runtime_migration: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub(super) fn allocate_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }
}
