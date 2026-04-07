use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::vst_host::plugin::LoadedPlugin;

use super::node::{ChannelConfig, Connection, NodeId, NodeInternalState, NodeType, PortId};

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

struct LooperBuffer {
    data: Vec<Vec<f32>>,
    channels: usize,
    capacity: usize,
    write_pos: usize,
    len: usize,
    playback_pos: usize,
}

impl LooperBuffer {
    fn new(channels: usize, sample_rate: f64, max_seconds: f64) -> Self {
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

    fn record(&mut self, input: &[Vec<f32>], num_frames: usize) {
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

    fn overdub(&mut self, input: &[Vec<f32>], num_frames: usize) {
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

    fn read_and_advance(&mut self, output: &mut [Vec<f32>], num_frames: usize) {
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

    fn clear(&mut self) {
        for ch in &mut self.data {
            ch.fill(0.0);
        }
        self.write_pos = 0;
        self.len = 0;
        self.playback_pos = 0;
    }

    fn clone_empty(&self) -> Self {
        Self::new(self.channels, 48000.0, 120.0)
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

    looper_buffer: Mutex<Option<LooperBuffer>>,
    metronome_phase: Mutex<f64>,
    metronome_click_remaining: Mutex<usize>,
}

impl Clone for GraphNode {
    fn clone(&self) -> Self {
        let looper_clone = if let Some(ref buf) = *self.looper_buffer.lock() {
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
            metronome_phase: Mutex::new(*self.metronome_phase.lock()),
            metronome_click_remaining: Mutex::new(*self.metronome_click_remaining.lock()),
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
            }),
            NodeType::Looper => NodeInternalState::Looper(super::node::LooperNodeState {
                enabled: false,
                recording: false,
                playing: false,
                overdubbing: false,
                cleared: false,
            }),
            NodeType::Gain => NodeInternalState::Gain { value: 1.0 },
            NodeType::Pan => NodeInternalState::Pan { value: 0.0 },
            NodeType::WetDry => NodeInternalState::WetDry { mix: 1.0 },
            NodeType::SendBus { .. } => NodeInternalState::SendBus { send_level: 1.0 },
            _ => NodeInternalState::None,
        };

        let looper_buffer = if matches!(node_type, NodeType::Looper) {
            let out_port = output_ports.first();
            let ch = out_port.map(|p| p.channels.channel_count()).unwrap_or(2);
            Mutex::new(Some(LooperBuffer::new(ch, 48000.0, 120.0)))
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
            metronome_phase: Mutex::new(0.0),
            metronome_click_remaining: Mutex::new(0),
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

pub struct AudioGraph {
    nodes: HashMap<NodeId, GraphNode>,
    connections: Vec<Connection>,

    process_order: Vec<NodeId>,

    input_node_id: Option<NodeId>,
    output_node_id: Option<NodeId>,

    max_frames: usize,
    sample_rate: f64,

    topology_dirty: bool,

    next_node_id: u64,
}

impl Clone for AudioGraph {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.iter().map(|(&id, n)| (id, n.clone())).collect(),
            connections: self.connections.clone(),
            process_order: self.process_order.clone(),
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
            input_node_id: None,
            output_node_id: None,
            max_frames,
            sample_rate,
            topology_dirty: true,
            next_node_id: 1,
        }
    }

    fn allocate_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn add_node(&mut self, node_type: NodeType) -> Result<NodeId, GraphError> {
        if node_type.is_singleton() {
            if matches!(node_type, NodeType::AudioInput) && self.input_node_id.is_some() {
                return Err(GraphError::SingletonViolation);
            }
            if matches!(node_type, NodeType::AudioOutput) && self.output_node_id.is_some() {
                return Err(GraphError::SingletonViolation);
            }
        }

        let id = self.allocate_node_id();
        let node = GraphNode::new(id, node_type.clone(), self.max_frames);

        match node_type {
            NodeType::AudioInput => self.input_node_id = Some(id),
            NodeType::AudioOutput => self.output_node_id = Some(id),
            _ => {}
        }

        self.nodes.insert(id, node);
        self.topology_dirty = true;
        Ok(id)
    }

    pub fn add_node_with_id(&mut self, id: NodeId, node_type: NodeType) -> Result<(), GraphError> {
        if node_type.is_singleton() {
            if matches!(node_type, NodeType::AudioInput) && self.input_node_id.is_some() {
                return Err(GraphError::SingletonViolation);
            }
            if matches!(node_type, NodeType::AudioOutput) && self.output_node_id.is_some() {
                return Err(GraphError::SingletonViolation);
            }
        }
        if self.nodes.contains_key(&id) {
            return Err(GraphError::SingletonViolation);
        }
        let node = GraphNode::new(id, node_type.clone(), self.max_frames);
        match node_type {
            NodeType::AudioInput => self.input_node_id = Some(id),
            NodeType::AudioOutput => self.output_node_id = Some(id),
            _ => {}
        }
        self.next_node_id = self.next_node_id.max(id.0 + 1);
        self.nodes.insert(id, node);
        self.topology_dirty = true;
        Ok(())
    }

    pub fn remove_node(&mut self, id: NodeId) {
        if self.nodes.remove(&id).is_some() {
            if self.input_node_id == Some(id) {
                self.input_node_id = None;
            }
            if self.output_node_id == Some(id) {
                self.output_node_id = None;
            }
            self.connections
                .retain(|c| c.source_node != id && c.target_node != id);
            self.topology_dirty = true;
        }
    }

    pub fn connect(&mut self, conn: Connection) -> Result<(), GraphError> {
        let source_node = self
            .nodes
            .get(&conn.source_node)
            .ok_or(GraphError::NotFound)?;
        let source_port = source_node
            .output_ports
            .iter()
            .find(|p| p.id == conn.source_port)
            .ok_or(GraphError::NotFound)?;

        let target_node = self
            .nodes
            .get(&conn.target_node)
            .ok_or(GraphError::NotFound)?;
        let target_port = target_node
            .input_ports
            .iter()
            .find(|p| p.id == conn.target_port)
            .ok_or(GraphError::NotFound)?;

        let existing: Vec<&Connection> = self
            .connections
            .iter()
            .filter(|c| c.target_node == conn.target_node && c.target_port == conn.target_port)
            .collect();

        if !existing.is_empty() {
            if existing
                .iter()
                .any(|c| c.source_node == conn.source_node && c.source_port == conn.source_port)
            {
                return Err(GraphError::AlreadyConnected);
            }
        }

        let source_ch = source_port.channels;
        let target_ch = target_port.channels;
        if source_ch != target_ch
            && !matches!(
                (&source_ch, &target_ch),
                (ChannelConfig::Mono, ChannelConfig::Stereo)
                    | (ChannelConfig::Stereo, ChannelConfig::Mono)
            )
        {
            return Err(GraphError::ChannelMismatch {
                source: source_ch,
                target: target_ch,
            });
        }

        let test_conn = conn.clone();
        self.connections.push(test_conn);

        if self.would_create_cycle(&conn) {
            self.connections.pop();
            return Err(GraphError::CycleDetected);
        }

        self.topology_dirty = true;
        Ok(())
    }

    pub fn disconnect(&mut self, source: (NodeId, PortId), target: (NodeId, PortId)) {
        let before = self.connections.len();
        self.connections.retain(|c| {
            !(c.source_node == source.0
                && c.source_port == source.1
                && c.target_node == target.0
                && c.target_port == target.1)
        });
        if self.connections.len() != before {
            self.topology_dirty = true;
        }
    }

    pub fn set_node_enabled(&mut self, id: NodeId, enabled: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.enabled = enabled;
        }
    }

    pub fn set_node_bypassed(&mut self, id: NodeId, bypassed: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.bypassed = bypassed;
        }
    }

    pub fn set_node_position(&mut self, id: NodeId, x: f32, y: f32) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.position = (x, y);
        }
    }

    pub fn set_node_internal_state(&mut self, id: NodeId, state: NodeInternalState) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.internal_state = state;
        }
    }

    pub fn commit_topology(&mut self) -> Result<(), GraphError> {
        if !self.topology_dirty {
            return Ok(());
        }

        let order = self.topological_sort()?;
        self.process_order = order;
        let mf = self.max_frames;
        for node in self.nodes.values() {
            let mut output_buffers = node.output_buffers.lock();
            for port_buf in output_buffers.iter_mut() {
                for ch_buf in port_buf.iter_mut() {
                    ch_buf.resize(mf, 0.0);
                }
            }
        }
        self.topology_dirty = false;
        Ok(())
    }

    fn topological_sort(&self) -> Result<Vec<NodeId>, GraphError> {
        let node_ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        let n = node_ids.len();

        let mut index_map: HashMap<NodeId, usize> = HashMap::new();
        for (i, &id) in node_ids.iter().enumerate() {
            index_map.insert(id, i);
        }

        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for conn in &self.connections {
            let &src_idx = index_map
                .get(&conn.source_node)
                .ok_or(GraphError::NotFound)?;
            let &tgt_idx = index_map
                .get(&conn.target_node)
                .ok_or(GraphError::NotFound)?;
            adj[src_idx].push(tgt_idx);
            in_degree[tgt_idx] += 1;
        }

        let mut queue: VecDeque<usize> = VecDeque::new();
        for (i, &deg) in in_degree.iter().enumerate() {
            if deg == 0 {
                queue.push_back(i);
            }
        }

        let mut result = Vec::with_capacity(n);
        while let Some(idx) = queue.pop_front() {
            result.push(node_ids[idx]);
            for &neighbor in &adj[idx] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        if result.len() != n {
            return Err(GraphError::CycleDetected);
        }

        Ok(result)
    }

    fn would_create_cycle(&self, conn: &Connection) -> bool {
        let mut visited = HashSet::new();
        let mut stack = VecDeque::new();
        stack.push_back(conn.target_node);

        while let Some(node_id) = stack.pop_front() {
            if node_id == conn.source_node {
                return true;
            }
            if visited.insert(node_id) {
                for c in &self.connections {
                    if c.source_node == node_id {
                        stack.push_back(c.target_node);
                    }
                }
            }
        }

        false
    }

    fn process_internal(&self, input: &[Vec<f32>], num_frames: usize) {
        if self.topology_dirty {
            return;
        }

        for &node_id in &self.process_order {
            let node = self.nodes.get(&node_id).unwrap();
            node.clear_output_buffers();
        }

        if let Some(input_id) = self.input_node_id {
            let input_node = self.nodes.get(&input_id).unwrap();
            let mut output_buffers = input_node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let ch_count = out_buf.len().min(input.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(out_buf[ch].len()).min(input[ch].len());
                    out_buf[ch][..copy_len].copy_from_slice(&input[ch][..copy_len]);
                }
            }
        }

        let process_order = self.process_order.clone();
        for &node_id in &process_order {
            self.gather_inputs(node_id, num_frames);

            let (enabled, bypassed) = {
                let node = self.nodes.get(&node_id).unwrap();
                (node.enabled, node.bypassed)
            };

            if !enabled {
                continue;
            }

            if bypassed {
                self.bypass_node(node_id, num_frames);
                continue;
            }

            self.process_node(node_id, num_frames);
        }
    }

    pub fn process(&self, input: &[Vec<f32>], num_frames: usize) -> Vec<Vec<f32>> {
        self.process_internal(input, num_frames);

        let output_id = self.output_node_id;
        if let Some(output_id) = output_id {
            let output_node = self.nodes.get(&output_id).unwrap();
            let input_buffers = output_node.input_buffers.lock();
            if let Some(buf) = input_buffers.get(0).and_then(|opt| opt.as_ref()) {
                let ch_count = buf.len().min(2);
                let mut result = vec![vec![0.0f32; num_frames]; 2];
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(buf[ch].len());
                    result[ch][..copy_len].copy_from_slice(&buf[ch][..copy_len]);
                }
                if ch_count == 1 {
                    let (left, right) = result.split_at_mut(1);
                    right[0][..num_frames].copy_from_slice(&left[0][..num_frames]);
                }
                return result;
            }
        }

        vec![vec![0.0f32; num_frames]; 2]
    }

    #[allow(dead_code)]
    pub fn process_into(&self, input: &[Vec<f32>], output: &mut [Vec<f32>], num_frames: usize) {
        for ch in output.iter_mut() {
            let len = num_frames.min(ch.len());
            ch[..len].fill(0.0);
        }

        self.process_internal(input, num_frames);

        let output_id = self.output_node_id;
        if let Some(output_id) = output_id {
            let output_node = self.nodes.get(&output_id).unwrap();
            let input_buffers = output_node.input_buffers.lock();
            if let Some(buf) = input_buffers.get(0).and_then(|opt| opt.as_ref()) {
                let ch_count = buf.len().min(output.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(output[ch].len()).min(buf[ch].len());
                    output[ch][..copy_len].copy_from_slice(&buf[ch][..copy_len]);
                }
                if ch_count == 1 && output.len() >= 2 {
                    let copy_len = num_frames.min(output[0].len()).min(buf[0].len());
                    output[1][..copy_len].copy_from_slice(&buf[0][..copy_len]);
                }
            }
        }
    }

    fn gather_inputs(&self, node_id: NodeId, num_frames: usize) {
        let max_frames = self.max_frames;

        let incoming: Vec<(PortId, Vec<(NodeId, PortId)>)> = {
            let mut port_map: HashMap<PortId, Vec<(NodeId, PortId)>> = HashMap::new();
            for conn in &self.connections {
                if conn.target_node == node_id {
                    port_map
                        .entry(conn.target_port)
                        .or_default()
                        .push((conn.source_node, conn.source_port));
                }
            }
            port_map.into_iter().collect()
        };

        let input_ports_info: Vec<(usize, usize)> = {
            let node = self.nodes.get(&node_id).unwrap();
            node.input_ports
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.channels.channel_count()))
                .collect()
        };

        let mut new_input_buffers: Vec<Option<Vec<Vec<f32>>>> = vec![None; input_ports_info.len()];

        for (target_port_id, sources) in incoming {
            let port_idx = match input_ports_info.iter().position(|(i, _)| {
                let node = self.nodes.get(&node_id).unwrap();
                node.input_ports
                    .get(*i)
                    .map(|p| p.id == target_port_id)
                    .unwrap_or(false)
            }) {
                Some(idx) => input_ports_info[idx].0,
                None => continue,
            };

            let target_ch = input_ports_info
                .iter()
                .find(|(i, _)| *i == port_idx)
                .map(|(_, ch)| *ch)
                .unwrap_or(1);

            let mut mixed = vec![vec![0.0f32; max_frames]; target_ch];

            for (src_idx, (src_node_id, src_port_id)) in sources.iter().enumerate() {
                let src_buffers: Vec<Vec<f32>> = {
                    if let Some(src_node) = self.nodes.get(src_node_id) {
                        if let Some(pidx) = src_node
                            .output_ports
                            .iter()
                            .position(|p| p.id == *src_port_id)
                        {
                            let shared = src_node.shared_outputs.lock();
                            if let Some(ref sb) = shared.get(pidx).and_then(|opt| opt.as_ref()) {
                                sb.as_slice().to_vec()
                            } else {
                                drop(shared);
                                let output_buffers = src_node.output_buffers.lock();
                                if let Some(src_buf) = output_buffers.get(pidx) {
                                    src_buf.clone()
                                } else {
                                    continue;
                                }
                            }
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                };

                let src_ch_count = src_buffers.len();
                let target_ch_count = mixed.len();

                if src_idx == 0 {
                    if src_ch_count == target_ch_count {
                        for ch in 0..target_ch_count {
                            let copy_len =
                                num_frames.min(mixed[ch].len()).min(src_buffers[ch].len());
                            mixed[ch][..copy_len].copy_from_slice(&src_buffers[ch][..copy_len]);
                        }
                    } else if src_ch_count == 1 && target_ch_count == 2 {
                        let copy_len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        mixed[0][..copy_len].copy_from_slice(&src_buffers[0][..copy_len]);
                        mixed[1][..copy_len].copy_from_slice(&src_buffers[0][..copy_len]);
                    } else if src_ch_count == 2 && target_ch_count == 1 {
                        let copy_len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..copy_len {
                            mixed[0][i] = (src_buffers[0][i] + src_buffers[1][i]) * 0.5;
                        }
                    }
                } else {
                    if src_ch_count == target_ch_count {
                        for ch in 0..target_ch_count {
                            let len = num_frames.min(mixed[ch].len()).min(src_buffers[ch].len());
                            for i in 0..len {
                                mixed[ch][i] += src_buffers[ch][i];
                            }
                        }
                    } else if src_ch_count == 1 && target_ch_count == 2 {
                        let len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..len {
                            mixed[0][i] += src_buffers[0][i];
                            mixed[1][i] += src_buffers[0][i];
                        }
                    } else if src_ch_count == 2 && target_ch_count == 1 {
                        let len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..len {
                            mixed[0][i] += (src_buffers[0][i] + src_buffers[1][i]) * 0.5;
                        }
                    }
                }
            }

            if port_idx < new_input_buffers.len() {
                new_input_buffers[port_idx] = Some(mixed);
            }
        }

        let node = self.nodes.get(&node_id).unwrap();
        {
            let ports = &node.input_ports;
            for (i, port) in ports.iter().enumerate() {
                if i >= new_input_buffers.len() {
                    break;
                }
                if new_input_buffers[i].is_none() {
                    let ch_count = port.channels.channel_count();
                    new_input_buffers[i] = Some(vec![vec![0.0f32; max_frames]; ch_count]);
                }
            }
        }
        *node.input_buffers.lock() = new_input_buffers;
    }

    fn process_node(&self, node_id: NodeId, num_frames: usize) {
        let node_type: NodeType = {
            let node = self.nodes.get(&node_id).unwrap();
            node.node_type.clone()
        };

        match &node_type {
            NodeType::AudioInput | NodeType::AudioOutput => {}
            NodeType::Pan => {
                self.process_pan_node(node_id, num_frames);
            }
            NodeType::Gain => {
                self.process_gain_node(node_id, num_frames);
            }
            NodeType::Mixer { .. } => {
                self.process_mixer_node(node_id, num_frames);
            }
            NodeType::Splitter { .. } => {
                self.process_splitter_node(node_id, num_frames);
            }
            NodeType::ChannelConverter { .. } => {
                self.process_converter_node(node_id, num_frames);
            }
            NodeType::Metronome => {
                self.process_metronome_node(node_id, num_frames);
            }
            NodeType::Looper => {
                self.process_looper_node(node_id, num_frames);
            }
            NodeType::VstPlugin { .. } => {
                self.process_vst_node(node_id, num_frames);
            }
            NodeType::WetDry => {
                self.process_wetdry_node(node_id, num_frames);
            }
            NodeType::SendBus { .. } => {
                self.process_send_bus_node(node_id, num_frames);
            }
            NodeType::ReturnBus { .. } => {
                self.process_return_bus_node(node_id, num_frames);
            }
        }
    }

    fn process_pan_node(&self, node_id: NodeId, num_frames: usize) {
        let pan_value = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Pan { value } => *value,
                _ => 0.0,
            }
        };

        let angle = (pan_value + 1.0) * std::f32::consts::FRAC_PI_4;
        let gain_l = angle.cos();
        let gain_r = angle.sin();

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
                if out_buf.len() >= 2 && !input_buf.is_empty() {
                    let copy_len = num_frames
                        .min(input_buf[0].len())
                        .min(out_buf[0].len())
                        .min(out_buf[1].len());
                    for i in 0..copy_len {
                        out_buf[0][i] = input_buf[0][i] * gain_l;
                        out_buf[1][i] = input_buf[0][i] * gain_r;
                    }
                }
            }
        }
    }

    fn process_gain_node(&self, node_id: NodeId, num_frames: usize) {
        let gain_value = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Gain { value } => *value,
                _ => 1.0,
            }
        };

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    for i in 0..copy_len {
                        out_buf[ch][i] = input_buf[ch][i] * gain_value;
                    }
                }
            }
        }
    }

    fn process_mixer_node(&self, node_id: NodeId, num_frames: usize) {
        let input_buffers_data: Vec<Option<Vec<Vec<f32>>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            node.input_buffers.lock().clone()
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
            for ch in 0..out_buf.len() {
                for i in 0..num_frames.min(out_buf[ch].len()) {
                    out_buf[ch][i] = 0.0;
                }
            }

            for input_buf_opt in &input_buffers_data {
                if let Some(input_buf) = input_buf_opt {
                    let ch_count = input_buf.len().min(out_buf.len());
                    for ch in 0..ch_count {
                        let len = num_frames.min(out_buf[ch].len()).min(input_buf[ch].len());
                        for i in 0..len {
                            out_buf[ch][i] += input_buf[ch][i];
                        }
                    }
                }
            }
        }
    }

    fn process_splitter_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut shared_outputs = node.shared_outputs.lock();
        let Some(input_data) = input_data else {
            return;
        };

        let num_outputs = output_buffers.len();
        if num_outputs == 0 {
            return;
        }

        {
            let first_buf = &mut output_buffers[0];
            let ch_count = input_data.len().min(first_buf.len());
            for ch in 0..ch_count {
                let copy_len = num_frames
                    .min(input_data[ch].len())
                    .min(first_buf[ch].len());
                first_buf[ch][..copy_len].copy_from_slice(&input_data[ch][..copy_len]);
            }
        }

        let shared = SharedBuffer::from_vec(output_buffers[0].clone());
        for i in 1..num_outputs {
            shared_outputs[i] = Some(shared.clone());
        }
    }

    fn process_converter_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let Some(input_data) = input_data else {
            return;
        };

        if let Some(out_buf) = output_buffers.get_mut(0) {
            let in_ch = input_data.len();
            let out_ch = out_buf.len();

            if in_ch == 1 && out_ch == 2 {
                let len = num_frames
                    .min(input_data[0].len())
                    .min(out_buf[0].len())
                    .min(out_buf[1].len());
                out_buf[0][..len].copy_from_slice(&input_data[0][..len]);
                out_buf[1][..len].copy_from_slice(&input_data[0][..len]);
            } else if in_ch == 2 && out_ch == 1 {
                let len = num_frames
                    .min(input_data[0].len())
                    .min(input_data[1].len())
                    .min(out_buf[0].len());
                for i in 0..len {
                    out_buf[0][i] = (input_data[0][i] + input_data[1][i]) * 0.5;
                }
            } else {
                let ch_count = in_ch.min(out_ch);
                for ch in 0..ch_count {
                    let len = num_frames.min(input_data[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_data[ch][..len]);
                }
            }
        }
    }

    fn process_metronome_node(&self, node_id: NodeId, num_frames: usize) {
        let (bpm, volume) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Metronome(state) => (state.bpm, state.volume),
                _ => (120.0, 0.5),
            }
        };

        let click_freq: f64 = 1000.0;
        let click_duration: usize = 480;
        let sample_rate = self.sample_rate;
        let samples_per_beat = sample_rate * 60.0 / bpm;

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut phase = node.metronome_phase.lock();
        let mut click_remaining = node.metronome_click_remaining.lock();

        if let Some(out_buf) = output_buffers.get_mut(0) {
            if out_buf.is_empty() {
                return;
            }
            let ch_count = out_buf.len();

            for frame in 0..num_frames {
                let sample = if *click_remaining > 0 {
                    let t = (click_duration - *click_remaining) as f64;
                    let val = (2.0 * std::f64::consts::PI * click_freq * t / sample_rate).sin()
                        * (*click_remaining as f64 / click_duration as f64);
                    *click_remaining -= 1;
                    val as f32 * volume
                } else {
                    0.0
                };

                for ch in 0..ch_count {
                    if frame < out_buf[ch].len() {
                        out_buf[ch][frame] = sample;
                    }
                }

                *phase += 1.0;
                if *phase >= samples_per_beat {
                    *phase -= samples_per_beat;
                    *click_remaining = click_duration;
                }
            }
        }
    }

    fn process_looper_node(&self, node_id: NodeId, num_frames: usize) {
        let state: super::node::LooperNodeState = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Looper(s) => s.clone(),
                _ => return,
            }
        };

        if !state.enabled {
            return;
        }

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();

        if state.recording {
            if let Some(ref input_buf) = input_data {
                let mut looper = node.looper_buffer.lock();
                if let Some(ref mut buf) = *looper {
                    buf.record(input_buf, num_frames);
                }
            }
        }

        if state.playing {
            let mut looper = node.looper_buffer.lock();
            if let Some(ref mut buf) = *looper {
                if let Some(out_buf) = output_buffers.get_mut(0) {
                    let ch_count = out_buf.len();
                    let mut temp_out = vec![vec![0.0f32; num_frames]; ch_count];
                    buf.read_and_advance(&mut temp_out, num_frames);
                    for ch in 0..ch_count {
                        let len = num_frames.min(out_buf[ch].len()).min(temp_out[ch].len());
                        for i in 0..len {
                            out_buf[ch][i] += temp_out[ch][i];
                        }
                    }
                }
            }
        } else if let Some(ref input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
        }

        if state.overdubbing {
            if let Some(ref input_buf) = input_data {
                let mut looper = node.looper_buffer.lock();
                if let Some(ref mut buf) = *looper {
                    buf.overdub(input_buf, num_frames);
                }
            }
        }
    }

    fn process_vst_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Vec<Vec<f32>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers
                .get(0)
                .and_then(|opt| opt.clone())
                .unwrap_or_default()
        };

        let num_ch = {
            let node = self.nodes.get(&node_id).unwrap();
            node.output_ports
                .first()
                .map(|p| p.channels.channel_count())
                .unwrap_or(0)
        };

        if num_ch == 0 {
            return;
        }

        let has_plugin = {
            let node = self.nodes.get(&node_id).unwrap();
            node.plugin_instance.lock().is_some()
        };

        if has_plugin {
            let mut temp_io: Vec<Vec<f32>> = vec![vec![0.0f32; num_frames]; num_ch];
            let ch_count = input_data.len().min(num_ch);
            for ch in 0..ch_count {
                let copy_len = num_frames.min(input_data[ch].len()).min(temp_io[ch].len());
                temp_io[ch][..copy_len].copy_from_slice(&input_data[ch][..copy_len]);
            }

            {
                let node = self.nodes.get(&node_id).unwrap();
                let mut plugin_instance = node.plugin_instance.lock();
                if let Some(ref mut plugin) = *plugin_instance {
                    let mut slices: Vec<&mut [f32]> =
                        temp_io.iter_mut().map(|v| &mut v[..]).collect();
                    plugin.process_in_place(&mut slices, num_frames as i32);
                }
            }

            let node = self.nodes.get(&node_id).unwrap();
            let mut output_buffers = node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let out_ch = out_buf.len().min(num_ch);
                for ch in 0..out_ch {
                    let len = num_frames.min(temp_io[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&temp_io[ch][..len]);
                }
            }
        } else {
            let node = self.nodes.get(&node_id).unwrap();
            let mut output_buffers = node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let out_ch = out_buf.len();
                let in_ch = input_data.len();
                let ch_count = in_ch.min(out_ch);
                for ch in 0..ch_count {
                    let len = num_frames.min(input_data[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_data[ch][..len]);
                }
            }
        }
    }

    fn bypass_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.first().and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.first_mut() {
                let in_ch = input_buf.len();
                let out_ch = out_buf.len();

                if in_ch == out_ch {
                    for ch in 0..in_ch {
                        let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                        out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                    }
                } else if in_ch == 1 && out_ch == 2 {
                    let len = num_frames
                        .min(input_buf[0].len())
                        .min(out_buf[0].len())
                        .min(out_buf[1].len());
                    out_buf[0][..len].copy_from_slice(&input_buf[0][..len]);
                    out_buf[1][..len].copy_from_slice(&input_buf[0][..len]);
                } else if in_ch == 2 && out_ch == 1 {
                    let len = num_frames
                        .min(input_buf[0].len())
                        .min(input_buf[1].len())
                        .min(out_buf[0].len());
                    for i in 0..len {
                        out_buf[0][i] = (input_buf[0][i] + input_buf[1][i]) * 0.5;
                    }
                }
            }
        }
    }

    pub fn get_node(&self, id: NodeId) -> Option<&GraphNode> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut GraphNode> {
        self.nodes.get_mut(&id)
    }

    #[allow(dead_code)]
    pub fn input_node_id(&self) -> Option<NodeId> {
        self.input_node_id
    }

    #[allow(dead_code)]
    pub fn output_node_id(&self) -> Option<NodeId> {
        self.output_node_id
    }

    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    pub fn nodes(&self) -> &HashMap<NodeId, GraphNode> {
        &self.nodes
    }

    #[allow(dead_code)]
    pub fn nodes_mut(&mut self) -> &mut HashMap<NodeId, GraphNode> {
        &mut self.nodes
    }

    #[allow(dead_code)]
    pub fn process_order(&self) -> &[NodeId] {
        &self.process_order
    }

    #[allow(dead_code)]
    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
    }

    #[allow(dead_code)]
    pub fn set_max_frames(&mut self, frames: usize) {
        self.max_frames = frames;
        for node in self.nodes.values() {
            node.resize_buffers(frames);
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    pub fn looper_loop_length(&self, node_id: NodeId) -> usize {
        let node = match self.nodes.get(&node_id) {
            Some(n) => n,
            None => return 0,
        };
        let looper = node.looper_buffer.lock();
        match *looper {
            Some(ref buf) => buf.len,
            None => 0,
        }
    }

    pub fn clear_looper(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get(&node_id) {
            let mut looper = node.looper_buffer.lock();
            if let Some(ref mut buf) = *looper {
                buf.clear();
            }
        }
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.internal_state = NodeInternalState::Looper(super::node::LooperNodeState {
                enabled: false,
                recording: false,
                playing: false,
                overdubbing: false,
                cleared: false,
            });
        }
    }

    pub fn init_looper_buffer(&mut self, node_id: NodeId) {
        let sample_rate = self.sample_rate;
        let ch = if let Some(node) = self.nodes.get(&node_id) {
            node.output_ports
                .first()
                .map(|p| p.channels.channel_count())
                .unwrap_or(2)
        } else {
            return;
        };
        if let Some(node) = self.nodes.get(&node_id) {
            let mut looper = node.looper_buffer.lock();
            *looper = Some(LooperBuffer::new(ch, sample_rate, 120.0));
        }
    }

    fn process_wetdry_node(&self, node_id: NodeId, num_frames: usize) {
        let mix = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::WetDry { mix } => *mix,
                _ => 0.5,
            }
        };

        let dry_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let wet_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(1).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
            let ch_count = out_buf.len();
            for ch in 0..ch_count {
                let len = num_frames.min(out_buf[ch].len());
                for i in 0..len {
                    let dry = dry_data
                        .as_ref()
                        .and_then(|d| d.get(ch).map(|c| c.get(i).copied().unwrap_or(0.0)))
                        .unwrap_or(0.0);
                    let wet = wet_data
                        .as_ref()
                        .and_then(|w| w.get(ch).map(|c| c.get(i).copied().unwrap_or(0.0)))
                        .unwrap_or(0.0);
                    out_buf[ch][i] = dry * (1.0 - mix) + wet * mix;
                }
            }
        }
    }

    fn process_send_bus_node(&self, node_id: NodeId, num_frames: usize) {
        let send_level = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::SendBus { send_level } => *send_level,
                _ => 1.0,
            }
        };

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();

        if let Some(ref input_buf) = input_data {
            if let Some(thru_buf) = output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(thru_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(thru_buf[ch].len());
                    thru_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
            if let Some(send_buf) = output_buffers.get_mut(1) {
                let ch_count = input_buf.len().min(send_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(send_buf[ch].len());
                    for i in 0..len {
                        send_buf[ch][i] = input_buf[ch][i] * send_level;
                    }
                }
            }
        }
    }

    fn process_return_bus_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
            if let Some(ref input_buf) = input_data {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::node::{Connection, NodeInternalState, NodeType, PortId};

    #[test]
    fn test_add_audio_input_output() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        assert_eq!(graph.input_node_id(), Some(input_id));
        assert_eq!(graph.output_node_id(), Some(output_id));
    }

    #[test]
    fn test_singleton_violation() {
        let mut graph = AudioGraph::new(48000.0, 256);
        graph.add_node(NodeType::AudioInput).unwrap();
        let result = graph.add_node(NodeType::AudioInput);
        assert!(matches!(result, Err(GraphError::SingletonViolation)));
    }

    #[test]
    fn test_connect_and_topology() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();
        let order = graph.process_order();

        assert_eq!(order.len(), 3);
        assert_eq!(order[0], input_id);
        assert_eq!(order[1], gain_id);
        assert_eq!(order[2], output_id);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let a = graph.add_node(NodeType::Gain).unwrap();
        let b = graph.add_node(NodeType::Gain).unwrap();

        graph
            .connect(Connection {
                source_node: a,
                source_port: PortId(0),
                target_node: b,
                target_port: PortId(0),
            })
            .unwrap();

        let result = graph.connect(Connection {
            source_node: b,
            source_port: PortId(0),
            target_node: a,
            target_port: PortId(0),
        });

        assert!(matches!(result, Err(GraphError::CycleDetected)));
    }

    #[test]
    fn test_remove_node_cleans_connections() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        assert_eq!(graph.connections().len(), 1);
        graph.remove_node(gain_id);
        assert!(graph.connections().is_empty());
    }

    #[test]
    fn test_process_simple_chain() {
        let mut graph = AudioGraph::new(48000.0, 256);
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

        let input = vec![vec![0.5f32; 256]];
        let output = graph.process(&input, 256);

        assert_eq!(output.len(), 2);
        assert_eq!(output[0].len(), 256);
        assert_eq!(output[1].len(), 256);
    }

    #[test]
    fn test_process_with_gain() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let gain_node = graph.get_node_mut(gain_id).unwrap();
            gain_node.internal_state = NodeInternalState::Gain { value: 0.5 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            assert!((output[0][i] - 0.5).abs() < 0.001);
            assert!((output[1][i] - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn test_pan_node() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let pan_id = graph.add_node(NodeType::Pan).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let pan_node = graph.get_node_mut(pan_id).unwrap();
            pan_node.internal_state = NodeInternalState::Pan { value: 1.0 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: pan_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: pan_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..10 {
            let l = output[0][i];
            let r = output[1][i];
            assert!(r > l, "Full right pan: R({}) should be > L({})", r, l);
        }
    }

    #[test]
    fn test_splitter_mixer_parallel() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
        let gain_a_id = graph.add_node(NodeType::Gain).unwrap();
        let gain_b_id = graph.add_node(NodeType::Gain).unwrap();
        let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let node = graph.get_node_mut(gain_a_id).unwrap();
            node.internal_state = NodeInternalState::Gain { value: 0.5 };
        }
        {
            let node = graph.get_node_mut(gain_b_id).unwrap();
            node.internal_state = NodeInternalState::Gain { value: 0.3 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: splitter_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(0),
                target_node: gain_a_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(1),
                target_node: gain_b_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_a_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_b_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(1),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: mixer_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            let expected = 0.5 + 0.3;
            assert!(
                (output[0][i] - expected).abs() < 0.001,
                "Expected {} but got {} at frame {}",
                expected,
                output[0][i],
                i
            );
        }
    }

    #[test]
    fn test_mono_stereo_auto_conversion_allowed() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let pan_id = graph.add_node(NodeType::Pan).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();

        let result = graph.connect(Connection {
            source_node: pan_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        });

        assert!(
            result.is_ok(),
            "Stereo->Mono auto-conversion should be allowed"
        );
    }

    #[test]
    fn test_disconnect() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        assert_eq!(graph.connections().len(), 1);

        graph.disconnect((input_id, PortId(0)), (gain_id, PortId(0)));
        assert!(graph.connections().is_empty());
    }

    #[test]
    fn test_bypass_node() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let gain_node = graph.get_node_mut(gain_id).unwrap();
            gain_node.internal_state = NodeInternalState::Gain { value: 0.0 };
            gain_node.bypassed = true;
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            assert!(
                (output[0][i] - 1.0).abs() < 0.001,
                "Bypassed gain should pass through: got {}",
                output[0][i]
            );
        }
    }

    #[test]
    fn test_disabled_node() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let gain_node = graph.get_node_mut(gain_id).unwrap();
            gain_node.enabled = false;
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            assert!(
                (output[0][i]).abs() < 0.001,
                "Disabled node should output silence: got {}",
                output[0][i]
            );
        }
    }

    #[test]
    fn test_set_node_position() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let id = graph.add_node(NodeType::Gain).unwrap();
        graph.set_node_position(id, 100.0, 200.0);
        let node = graph.get_node(id).unwrap();
        assert_eq!(node.position, (100.0, 200.0));
    }

    #[test]
    fn test_set_node_enabled_bypassed() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let id = graph.add_node(NodeType::Gain).unwrap();
        assert!(graph.get_node(id).unwrap().enabled);
        assert!(!graph.get_node(id).unwrap().bypassed);

        graph.set_node_enabled(id, false);
        assert!(!graph.get_node(id).unwrap().enabled);

        graph.set_node_bypassed(id, true);
        assert!(graph.get_node(id).unwrap().bypassed);
    }

    #[test]
    fn test_already_connected() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();

        let conn = Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        };
        graph.connect(conn.clone()).unwrap();
        let result = graph.connect(conn);
        assert!(matches!(result, Err(GraphError::AlreadyConnected)));
    }

    #[test]
    fn test_connect_nonexistent_node() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let result = graph.connect(Connection {
            source_node: NodeId(999),
            source_port: PortId(0),
            target_node: NodeId(998),
            target_port: PortId(0),
        });
        assert!(matches!(result, Err(GraphError::NotFound)));
    }

    #[test]
    fn test_wetdry_node() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
        let wetdry_id = graph.add_node(NodeType::WetDry).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let node = graph.get_node_mut(wetdry_id).unwrap();
            node.internal_state = NodeInternalState::WetDry { mix: 0.5 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: splitter_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(0),
                target_node: wetdry_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(1),
                target_node: wetdry_id,
                target_port: PortId(1),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: wetdry_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![0.8f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            assert!(
                (output[0][i] - 0.8).abs() < 0.001,
                "Wet/Dry mix=0.5 should output ~0.8 at sample {}",
                i
            );
            assert!(
                (output[1][i] - 0.8).abs() < 0.001,
                "Wet/Dry mix=0.5 should output ~0.8 at sample {}",
                i
            );
        }
    }

    #[test]
    fn test_wetdry_full_wet() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        {
            let node = graph.get_node_mut(gain_id).unwrap();
            node.internal_state = NodeInternalState::Gain { value: 0.5 };
        }
        let wetdry_id = graph.add_node(NodeType::WetDry).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let node = graph.get_node_mut(wetdry_id).unwrap();
            node.internal_state = NodeInternalState::WetDry { mix: 1.0 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: splitter_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(0),
                target_node: wetdry_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: splitter_id,
                source_port: PortId(1),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: wetdry_id,
                target_port: PortId(1),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: wetdry_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..256 {
            assert!(
                (output[0][i] - 0.5).abs() < 0.001,
                "Full wet should output gain*input=0.5 at sample {}",
                i
            );
        }
    }

    #[test]
    fn test_send_return_bus() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let converter_id = graph
            .add_node(NodeType::ChannelConverter {
                target: ChannelConfig::Stereo,
            })
            .unwrap();
        let send_id = graph.add_node(NodeType::SendBus { bus_id: 1 }).unwrap();
        let return_id = graph.add_node(NodeType::ReturnBus { bus_id: 1 }).unwrap();
        let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let node = graph.get_node_mut(send_id).unwrap();
            node.internal_state = NodeInternalState::SendBus { send_level: 0.5 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: converter_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: converter_id,
                source_port: PortId(0),
                target_node: send_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: send_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: send_id,
                source_port: PortId(1),
                target_node: return_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: return_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(1),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: mixer_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        assert_eq!(output.len(), 2);
        assert_eq!(output[0].len(), 256);
        assert_eq!(output[1].len(), 256);

        for i in 0..10 {
            let thru_signal = 1.0;
            let send_signal = 0.5;
            let mixed = thru_signal + send_signal;
            assert!(
                (output[0][i] - mixed).abs() < 0.01,
                "Output should be thru+send at sample {}: got {}",
                i,
                output[0][i]
            );
        }
    }

    #[test]
    fn test_send_bus_zero_level() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let converter_id = graph
            .add_node(NodeType::ChannelConverter {
                target: ChannelConfig::Stereo,
            })
            .unwrap();
        let send_id = graph.add_node(NodeType::SendBus { bus_id: 1 }).unwrap();
        let return_id = graph.add_node(NodeType::ReturnBus { bus_id: 1 }).unwrap();
        let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        {
            let node = graph.get_node_mut(send_id).unwrap();
            node.internal_state = NodeInternalState::SendBus { send_level: 0.0 };
        }

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: converter_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: converter_id,
                source_port: PortId(0),
                target_node: send_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: send_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: send_id,
                source_port: PortId(1),
                target_node: return_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: return_id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(1),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: mixer_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.commit_topology().unwrap();

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);

        for i in 0..10 {
            assert!(
                (output[0][i] - 1.0).abs() < 0.01,
                "Zero send should give only thru signal at sample {}: got {}",
                i,
                output[0][i]
            );
        }
    }

    #[test]
    fn test_add_node_with_id() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let _ = graph.add_node(NodeType::AudioInput).unwrap();
        let _ = graph.add_node(NodeType::AudioOutput).unwrap();

        graph.add_node_with_id(NodeId(10), NodeType::Gain).unwrap();
        assert!(graph.get_node(NodeId(10)).is_some());

        let node = graph.get_node(NodeId(10)).unwrap();
        assert!(matches!(node.node_type, NodeType::Gain));
    }

    #[test]
    fn test_add_node_with_id_updates_next_id() {
        let mut graph = AudioGraph::new(48000.0, 256);
        graph.add_node_with_id(NodeId(100), NodeType::Gain).unwrap();
        let next = graph.add_node(NodeType::Pan).unwrap();
        assert!(next.0 > 100, "next_node_id should be > 100, got {}", next.0);
    }

    #[test]
    fn test_undo_remove_node_restore() {
        let mut graph = AudioGraph::new(48000.0, 256);
        let input_id = graph.add_node(NodeType::AudioInput).unwrap();
        let gain_id = graph.add_node(NodeType::Gain).unwrap();
        let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();

        graph.set_node_internal_state(gain_id, NodeInternalState::Gain { value: 1.5 });
        graph.set_node_position(gain_id, 100.0, 200.0);
        graph.commit_topology().unwrap();

        graph.remove_node(gain_id);
        assert!(graph.get_node(gain_id).is_none());
        assert_eq!(graph.connections().len(), 0);

        graph.add_node_with_id(gain_id, NodeType::Gain).unwrap();
        graph.set_node_position(gain_id, 100.0, 200.0);
        graph.set_node_internal_state(gain_id, NodeInternalState::Gain { value: 1.5 });
        graph
            .connect(Connection {
                source_node: input_id,
                source_port: PortId(0),
                target_node: gain_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph
            .connect(Connection {
                source_node: gain_id,
                source_port: PortId(0),
                target_node: output_id,
                target_port: PortId(0),
            })
            .unwrap();
        graph.commit_topology().unwrap();

        assert!(graph.get_node(gain_id).is_some());
        let node = graph.get_node(gain_id).unwrap();
        assert_eq!(node.position, (100.0, 200.0));
        assert!(matches!(
            node.internal_state,
            NodeInternalState::Gain { value: 1.5 }
        ));
        assert_eq!(graph.connections().len(), 2);

        let input = vec![vec![1.0f32; 256]];
        let output = graph.process(&input, 256);
        for i in 0..10 {
            assert!(
                (output[0][i] - 1.5).abs() < 0.01,
                "Restored gain node should apply gain at sample {}: got {}",
                i,
                output[0][i]
            );
        }
    }
}
