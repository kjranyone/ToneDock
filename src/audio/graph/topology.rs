use std::collections::{HashMap, HashSet, VecDeque};

use crate::audio::node::{ChannelConfig, Connection, NodeId, NodeType, PortId};

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
            return Err(GraphError::AlreadyConnected);
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
}
