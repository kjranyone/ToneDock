use std::collections::HashMap;

use crate::audio::node::{Connection, NodeId, NodeInternalState};

use super::{AudioGraph, GraphNode, LooperBuffer};

impl AudioGraph {
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
            node.internal_state =
                NodeInternalState::Looper(crate::audio::node::LooperNodeState {
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
}
