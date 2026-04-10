use super::helpers::commit_and_publish_graph;
use super::AudioEngine;

impl AudioEngine {
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
