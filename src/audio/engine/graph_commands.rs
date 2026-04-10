use crate::audio::graph_command::GraphCommand;
use crate::audio::node::{Connection, NodeId, NodeInternalState, NodeType, PortId};

use super::helpers::commit_and_publish_graph;
use super::AudioEngine;

impl AudioEngine {
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
                super::helpers::apply_command(&mut staging, cmd);
            }
            if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
                log::error!("Topology commit in staging failed: {}", e);
            }
        }
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

    pub(super) fn find_node_id_by_type(&self, target: NodeType) -> Option<NodeId> {
        let guard = self.graph.load();
        for (&id, node) in guard.nodes() {
            if std::mem::discriminant(&node.node_type) == std::mem::discriminant(&target) {
                return Some(id);
            }
        }
        None
    }
}
