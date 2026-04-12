use super::ToneDockApp;
use crate::audio::node::{Connection, NodeInternalState, NodeType};
use crate::ui::node_editor::EdCmd;
use crate::undo::{UndoAction, UndoStep};

impl ToneDockApp {
    pub(crate) fn process_editor_commands(&mut self, cmds: Vec<EdCmd>) {
        let mut undo_actions: Vec<UndoAction> = Vec::new();
        let mut is_continuous = false;

        for cmd in cmds {
            match cmd {
                EdCmd::AddNode(node_type, pos) => {
                    let id =
                        self.audio_engine
                            .add_node_with_position(node_type.clone(), pos.0, pos.1);
                    undo_actions.push(UndoAction::AddedNode {
                        node_id: id,
                        node_type,
                        position: pos,
                    });
                    self.status_message = self
                        .i18n
                        .trf("status.added_node", &[("id", &format!("{:?}", id))]);
                }
                EdCmd::AddVstNode {
                    plugin_path,
                    plugin_name,
                    pos,
                } => {
                    let node_type = NodeType::VstPlugin {
                        plugin_path: plugin_path.to_string_lossy().into_owned(),
                        plugin_name: plugin_name.clone(),
                    };
                    let id =
                        self.audio_engine
                            .add_node_with_position(node_type.clone(), pos.0, pos.1);
                    undo_actions.push(UndoAction::AddedNode {
                        node_id: id,
                        node_type,
                        position: pos,
                    });
                    match self.audio_engine.load_vst_plugin_to_node(
                        id,
                        &crate::vst_host::scanner::PluginInfo {
                            path: plugin_path.clone(),
                            name: plugin_name.clone(),
                            category: String::new(),
                            vendor: String::new(),
                        },
                    ) {
                        Ok(()) => {
                            self.node_editor.set_selection(Some(id));
                            self.status_message = self
                                .i18n
                                .trf("status.loaded_vst", &[("name", &plugin_name)]);
                        }
                        Err(e) => {
                            self.status_message = self
                                .i18n
                                .trf("status.vst_load_error", &[("error", &e.to_string())]);
                            log::error!("Failed to load VST plugin '{}': {}", plugin_name, e);
                        }
                    }
                }
                EdCmd::RemoveNode(id) => {
                    {
                        let guard = self.audio_engine.graph.load();
                        if let Some(node) = guard.get_node(id) {
                            let connections: Vec<Connection> = guard
                                .connections()
                                .iter()
                                .filter(|c| c.source_node == id || c.target_node == id)
                                .cloned()
                                .collect();
                            undo_actions.push(UndoAction::RemovedNode {
                                node_id: id,
                                node_type: node.node_type.clone(),
                                position: node.position,
                                enabled: node.enabled,
                                bypassed: node.bypassed,
                                state: node.internal_state.clone(),
                                connections,
                            });
                        }
                    }
                    if let Some(index) = self
                        .audio_engine
                        .chain_node_ids
                        .iter()
                        .position(|n| *n == id)
                    {
                        self.close_rack_editor(id);
                        self.audio_engine.chain_node_ids.remove(index);
                        if self.selected_rack_node == Some(id) {
                            self.select_rack_plugin_node(None);
                        }
                        self.rebuild_rack_signal_chain();
                    }
                    self.audio_engine.graph_remove_node(id);
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = self
                        .i18n
                        .trf("status.removed_node", &[("id", &format!("{:?}", id))]);
                }
                EdCmd::Connect(conn) => {
                    undo_actions.push(UndoAction::Connected(conn.clone()));
                    self.audio_engine.graph_connect(conn);
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = self.i18n.tr("status.connected").into();
                }
                EdCmd::Disconnect(src_node, src_port, tgt_node, tgt_port) => {
                    let conn = Connection {
                        source_node: src_node,
                        source_port: src_port,
                        target_node: tgt_node,
                        target_port: tgt_port,
                    };
                    undo_actions.push(UndoAction::Disconnected(conn));
                    self.audio_engine
                        .graph_disconnect((src_node, src_port), (tgt_node, tgt_port));
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = self.i18n.tr("status.disconnected").into();
                }
                EdCmd::SetPos(id, x, y) => {
                    let old_pos = {
                        let guard = self.audio_engine.graph.load();
                        guard.get_node(id).map(|n| n.position).unwrap_or((0.0, 0.0))
                    };
                    undo_actions.push(UndoAction::MovedNode {
                        node_id: id,
                        old_pos,
                        new_pos: (x, y),
                    });
                    self.audio_engine.send_command(
                        crate::audio::graph_command::GraphCommand::SetNodePosition(id, x, y),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::SetState(id, state) => {
                    let old_state = {
                        let guard = self.audio_engine.graph.load();
                        guard
                            .get_node(id)
                            .map(|n| n.internal_state.clone())
                            .unwrap_or(NodeInternalState::None)
                    };
                    if std::mem::discriminant(&old_state) == std::mem::discriminant(&state) {
                        is_continuous = true;
                    }
                    undo_actions.push(UndoAction::ChangedState {
                        node_id: id,
                        old_state,
                        new_state: state.clone(),
                    });
                    self.audio_engine.graph_set_state(id, state);
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::ToggleBypass(id) => {
                    let old_bypassed = {
                        let guard = self.audio_engine.graph.load();
                        guard.get_node(id).map(|n| n.bypassed).unwrap_or(false)
                    };
                    undo_actions.push(UndoAction::ChangedBypass {
                        node_id: id,
                        old_bypassed,
                        new_bypassed: !old_bypassed,
                    });
                    self.audio_engine.graph_set_bypassed(id, !old_bypassed);
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::DuplicateNode(id) => {
                    let guard = self.audio_engine.graph.load();
                    if let Some(node) = guard.get_node(id) {
                        let node_type = node.node_type.clone();
                        let state = node.internal_state.clone();
                        let (ox, oy) = node.position;
                        drop(guard);
                        let new_id = self.audio_engine.add_node_with_position(
                            node_type.clone(),
                            ox + 50.0,
                            oy + 50.0,
                        );
                        self.audio_engine.graph_set_state(new_id, state.clone());
                        self.audio_engine.graph_commit_topology();
                        self.audio_engine.apply_commands_to_staging();
                        undo_actions.push(UndoAction::AddedNode {
                            node_id: new_id,
                            node_type,
                            position: (ox + 50.0, oy + 50.0),
                        });
                        self.node_editor.set_selection(Some(new_id));
                        self.status_message = self.i18n.trf(
                            "status.duplicated_node",
                            &[("id", &format!("{:?}", new_id))],
                        );
                    }
                }
                EdCmd::SetVstParameter {
                    node_id,
                    param_index,
                    value,
                } => {
                    self.audio_engine
                        .set_vst_node_parameter(node_id, param_index, value);
                }
                EdCmd::Commit => {
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::ApplyTemplate(template_name, pos) => {
                    self.apply_template(&template_name, pos);
                }
            }
        }

        if !undo_actions.is_empty() {
            let label = if is_continuous {
                self.i18n.tr("undo.change_parameter").into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::AddedNode { .. }))
            {
                self.i18n.tr("undo.add_node").into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::RemovedNode { .. }))
            {
                self.i18n.tr("undo.remove_node").into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::Connected(_)))
            {
                self.i18n.tr("undo.connect").into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::Disconnected(_)))
            {
                self.i18n.tr("undo.disconnect").into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::MovedNode { .. }))
            {
                self.i18n.tr("undo.move_node").into()
            } else {
                self.i18n.tr("undo.edit").into()
            };

            self.undo_manager.push(UndoStep {
                label,
                actions: undo_actions,
                is_continuous,
            });
        }
    }

    pub(crate) fn perform_undo(&mut self) {
        if let Some(step) = self.undo_manager.pop_undo() {
            let actions: Vec<UndoAction> = step.actions.into_iter().rev().collect();
            self.audio_engine.execute_undo_actions(&actions);
            self.status_message = self.i18n.trf("status.undo", &[("label", &step.label)]);
        }
    }

    pub(crate) fn perform_redo(&mut self) {
        if let Some(step) = self.undo_manager.pop_redo() {
            self.audio_engine.execute_redo_actions(&step.actions);
            self.status_message = self.i18n.trf("status.redo", &[("label", &step.label)]);
        }
    }
}
