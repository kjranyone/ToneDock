use std::collections::{HashMap, HashSet, VecDeque};

use crate::audio::node::{
    ChannelConfig, Connection, NodeId, NodeType, Port, PortDirection, PortId,
};

use super::{AudioGraph, GraphError, GraphNode};

impl AudioGraph {
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
        let source_ch = source_node
            .output_ports
            .iter()
            .find(|p| p.id == conn.source_port)
            .ok_or(GraphError::NotFound)?
            .channels;

        self.ensure_mixer_port(conn.target_node, conn.target_port);

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
            return Err(GraphError::AlreadyConnected);
        }

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
        self.grow_mixer_ports(conn.target_node);
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
            self.shrink_mixer_ports(target.0);
        }
    }

    fn ensure_mixer_port(&mut self, node_id: NodeId, port_id: PortId) {
        let is_mixer = self
            .nodes
            .get(&node_id)
            .map(|n| matches!(n.node_type, NodeType::Mixer { .. }))
            .unwrap_or(false);
        if !is_mixer {
            return;
        }
        let node = self.nodes.get_mut(&node_id).unwrap();
        let target_idx = port_id.0 as usize;
        let buffers = node.buffers.get_mut();
        while node.input_ports.len() <= target_idx {
            let next_id = node.input_ports.len() as u32;
            node.input_ports.push(Port {
                id: PortId(next_id),
                name: format!("in_{}", next_id),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            });
            buffers.input_buffers.push(None);
        }
    }

    fn grow_mixer_ports(&mut self, node_id: NodeId) {
        let is_mixer = self
            .nodes
            .get(&node_id)
            .map(|n| matches!(n.node_type, NodeType::Mixer { .. }))
            .unwrap_or(false);
        if !is_mixer {
            return;
        }

        let all_connected = {
            let node = self.nodes.get(&node_id).unwrap();
            node.input_ports.iter().all(|port| {
                self.connections
                    .iter()
                    .any(|c| c.target_node == node_id && c.target_port == port.id)
            })
        };

        if all_connected {
            let node = self.nodes.get_mut(&node_id).unwrap();
            let next_id = node.input_ports.len() as u32;
            node.input_ports.push(Port {
                id: PortId(next_id),
                name: format!("in_{}", next_id),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            });
            let buffers = node.buffers.get_mut();
            buffers.input_buffers.push(None);
        }
    }

    fn shrink_mixer_ports(&mut self, node_id: NodeId) {
        let min_ports = self.nodes.get(&node_id).and_then(|n| match n.node_type {
            NodeType::Mixer { inputs } => Some((inputs as usize).max(1)),
            _ => None,
        });
        let Some(min_ports) = min_ports else {
            return;
        };

        let node = self.nodes.get_mut(&node_id).unwrap();
        let buffers = node.buffers.get_mut();
        while node.input_ports.len() > min_ports {
            let last = node.input_ports.last().unwrap();
            let last_connected = self
                .connections
                .iter()
                .any(|c| c.target_node == node_id && c.target_port == last.id);
            if last_connected {
                break;
            }
            let second_last_idx = node.input_ports.len() - 2;
            let second_last = &node.input_ports[second_last_idx];
            let second_last_connected = self
                .connections
                .iter()
                .any(|c| c.target_node == node_id && c.target_port == second_last.id);
            if second_last_connected {
                break;
            }
            node.input_ports.pop();
            buffers.input_buffers.pop();
        }
    }

    pub fn commit_topology(&mut self) -> Result<(), GraphError> {
        if !self.topology_dirty {
            return Ok(());
        }

        let order = self.topological_sort()?;
        self.process_order = order;

        let mut compiled = HashMap::new();
        for conn in &self.connections {
            let Some(target_node) = self.nodes.get(&conn.target_node) else {
                continue;
            };
            let Some(target_port_idx) = target_node
                .input_ports
                .iter()
                .position(|p| p.id == conn.target_port)
            else {
                continue;
            };
            let Some(source_node) = self.nodes.get(&conn.source_node) else {
                continue;
            };
            let Some(source_port_idx) = source_node
                .output_ports
                .iter()
                .position(|p| p.id == conn.source_port)
            else {
                continue;
            };
            compiled
                .entry(conn.target_node)
                .or_insert_with(Vec::new)
                .push(super::CompiledConnection {
                    source_node: conn.source_node,
                    source_port_idx,
                    target_port_idx,
                });
        }
        self.compiled_connections = compiled;

        self.id_to_index.clear();
        self.nodes_vec = self
            .process_order
            .iter()
            .enumerate()
            .map(|(i, &id)| {
                self.id_to_index.insert(id, i);
                self.nodes.get(&id).cloned().unwrap()
            })
            .collect();

        self.input_node_idx = self
            .input_node_id
            .and_then(|id| self.id_to_index.get(&id).copied());
        self.output_node_idx = self
            .output_node_id
            .and_then(|id| self.id_to_index.get(&id).copied());
        self.metronome_idx = None;
        for node in self.nodes_vec.iter() {
            if matches!(node.node_type, NodeType::Metronome) {
                self.metronome_idx = self.id_to_index.get(&node.id).copied();
                break;
            }
        }

        let n = self.nodes_vec.len();
        self.compiled_connections_vec = vec![Vec::new(); n];
        for (&id, conns) in &self.compiled_connections {
            let Some(&target_idx) = self.id_to_index.get(&id) else {
                continue;
            };
            let compiled_idx: Vec<super::CompiledConnectionIdx> = conns
                .iter()
                .filter_map(|cc| {
                    self.id_to_index.get(&cc.source_node).map(|&src_idx| {
                        super::CompiledConnectionIdx {
                            source_idx: src_idx,
                            source_port_idx: cc.source_port_idx,
                            target_port_idx: cc.target_port_idx,
                        }
                    })
                })
                .collect();
            self.compiled_connections_vec[target_idx] = compiled_idx;
        }

        let mf = self.max_frames;
        for node in self.nodes_vec.iter() {
            let b = node.buffers_mut();
            for port_buf in b.output_buffers.iter_mut() {
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
}
