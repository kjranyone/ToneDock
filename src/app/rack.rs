use super::ToneDockApp;
use crate::audio::node::NodeId;
use crate::audio::node::{Connection, NodeType, PortId};
use crate::ui::rack_view::RackSlotView;
use crate::vst_host::editor::PluginEditor;
use crate::vst_host::scanner::PluginInfo;

impl ToneDockApp {
    pub(crate) fn rack_node_position(index: usize) -> (f32, f32) {
        (120.0, 80.0 + index as f32 * 140.0)
    }

    pub(crate) fn discover_serial_rack_chain(
        graph: &crate::audio::graph::AudioGraph,
        mixer_id: NodeId,
    ) -> Option<Vec<NodeId>> {
        let input_id = graph.input_node_id()?;
        let mut current = input_id;
        let mut chain = Vec::new();

        loop {
            let next_vst: Vec<_> = graph
                .connections()
                .iter()
                .filter(|conn| conn.source_node == current)
                .filter(|conn| {
                    conn.target_node == mixer_id
                        || graph
                            .get_node(conn.target_node)
                            .map(|n| matches!(n.node_type, NodeType::VstPlugin { .. }))
                            .unwrap_or(false)
                })
                .collect();
            if next_vst.len() != 1 {
                return None;
            }

            let next_node_id = next_vst[0].target_node;
            if next_node_id == mixer_id {
                return Some(chain);
            }

            chain.push(next_node_id);
            current = next_node_id;
        }
    }

    pub(crate) fn rebuild_rack_projection_from_graph(&mut self) {
        let ordered_ids = {
            let guard = self.audio_engine.graph.load();
            Self::discover_serial_rack_chain(&guard, self.audio_engine.master_mixer_id)
                .unwrap_or_else(|| {
                    self.audio_engine
                        .chain_node_ids
                        .iter()
                        .copied()
                        .filter(|node_id| {
                            guard.get_node(*node_id).is_some_and(|node| {
                                matches!(node.node_type, NodeType::VstPlugin { .. })
                            })
                        })
                        .collect()
                })
        };

        self.audio_engine.chain_node_ids = ordered_ids;
        self.rack
            .order
            .retain(|node_id: &NodeId| self.audio_engine.chain_node_ids.contains(node_id));
        for node_id in &self.audio_engine.chain_node_ids {
            if !self.rack.order.contains(node_id) {
                self.rack.order.push(*node_id);
            }
        }
        self.rack
            .plugin_editors
            .retain(|node_id: &NodeId, editor: &mut PluginEditor| {
                self.rack.order.contains(node_id) && editor.is_open()
            });

        if self
            .rack
            .selected_node
            .is_some_and(|node_id| !self.rack.order.contains(&node_id))
        {
            self.rack.selected_node = None;
            self.rack_view.selected_plugin = None;
        }
    }

    pub(crate) fn select_rack_plugin_node(&mut self, node_id: Option<NodeId>) {
        self.rack.selected_node = node_id;
        self.rack_view.selected_plugin = node_id;
        self.node_editor.set_selection(node_id);
    }

    pub(crate) fn rebuild_rack_signal_chain(&mut self) {
        let chain_node_ids = self.audio_engine.chain_node_ids.clone();
        let guard = self.audio_engine.graph.load();
        let input_id = self.audio_engine.input_node_id;
        let mixer_id = self.audio_engine.master_mixer_id;
        let managed: std::collections::HashSet<NodeId> = std::iter::once(input_id)
            .chain(chain_node_ids.iter().copied())
            .chain(std::iter::once(mixer_id))
            .collect();
        let managed_connections: Vec<_> = guard
            .connections()
            .iter()
            .filter(|conn| {
                managed.contains(&conn.source_node) && managed.contains(&conn.target_node)
            })
            .cloned()
            .collect();
        drop(guard);

        for conn in managed_connections {
            self.audio_engine.graph_disconnect(
                (conn.source_node, conn.source_port),
                (conn.target_node, conn.target_port),
            );
        }

        let mut previous = input_id;
        for node_id in &chain_node_ids {
            self.audio_engine.graph_connect(Connection {
                source_node: previous,
                source_port: PortId(0),
                target_node: *node_id,
                target_port: PortId(0),
            });
            previous = *node_id;
        }

        self.audio_engine.graph_connect(Connection {
            source_node: previous,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(0),
        });
        self.audio_engine.graph_commit_topology();
        self.audio_engine.apply_commands_to_staging();
    }

    pub(crate) fn add_rack_plugin_to_graph(&mut self, info: &PluginInfo) -> anyhow::Result<NodeId> {
        let node_type = NodeType::VstPlugin {
            plugin_path: info.path.to_string_lossy().into_owned(),
            plugin_name: info.name.clone(),
        };
        let index = self.audio_engine.chain_node_ids.len();
        let (x, y) = Self::rack_node_position(index);
        let node_id = self.audio_engine.add_node_with_position(node_type, x, y);
        self.audio_engine.load_vst_plugin_to_node(node_id, info)?;
        self.audio_engine.chain_node_ids.push(node_id);
        self.rack.order.push(node_id);
        self.rebuild_rack_signal_chain();
        Ok(node_id)
    }

    pub(crate) fn remove_rack_plugin_from_graph(&mut self, node_id: NodeId) {
        let Some(index) = self
            .audio_engine
            .chain_node_ids
            .iter()
            .position(|id| *id == node_id)
        else {
            return;
        };

        self.close_rack_editor(node_id);
        self.audio_engine.chain_node_ids.remove(index);
        self.rack.order.retain(|id| *id != node_id);
        if self.node_editor.selected_node() == Some(node_id) {
            self.node_editor.set_selection(None);
        }
        self.audio_engine.graph_remove_node(node_id);
        self.audio_engine.apply_commands_to_staging();
        self.rebuild_rack_signal_chain();
    }

    pub(crate) fn reorder_rack_plugin(&mut self, node_id: NodeId, target_index: usize) {
        let Some(index) = self.rack.order.iter().position(|id| *id == node_id) else {
            return;
        };
        if index == target_index || target_index >= self.rack.order.len() {
            return;
        }
        let node_id = self.rack.order.remove(index);
        self.rack.order.insert(target_index, node_id);
    }

    pub(crate) fn sync_rack_plugin_state(
        &mut self,
        node_id: NodeId,
        enabled: bool,
        bypassed: bool,
    ) {
        self.audio_engine.graph_set_enabled(node_id, enabled);
        self.audio_engine.graph_set_bypassed(node_id, bypassed);
        self.audio_engine.apply_commands_to_staging();
    }

    pub(crate) fn close_rack_editor(&mut self, node_id: NodeId) {
        if let Some(mut editor) = self.rack.plugin_editors.remove(&node_id) {
            editor.close();
        }
        if self.rack.inline_editor_node == Some(node_id) {
            self.rack.inline_editor_node = None;
        }
    }

    pub(crate) fn close_all_rack_editors(&mut self) {
        for (_, mut editor) in self.rack.plugin_editors.drain() {
            editor.close();
        }
        self.rack.inline_editor_node = None;
    }

    pub(crate) fn build_rack_slots(&mut self) -> Vec<RackSlotView> {
        self.rebuild_rack_projection_from_graph();

        let guard = self.audio_engine.graph.load();
        self.rack
            .order
            .iter()
            .filter_map(|node_id| {
                let node = guard.get_node(*node_id)?;
                let NodeType::VstPlugin {
                    plugin_path,
                    plugin_name,
                } = &node.node_type
                else {
                    return None;
                };

                let plugin_info = self
                    .available_plugins
                    .iter()
                    .find(|info| info.path.to_string_lossy() == plugin_path.as_str());
                let has_editor = guard
                    .with_plugin(*node_id, |p| p.has_editor())
                    .unwrap_or(false);
                let plugin_loaded = guard.with_plugin(*node_id, |_| ()).is_some();

                Some(RackSlotView {
                    node_id: *node_id,
                    name: plugin_name.clone(),
                    vendor: plugin_info
                        .map(|info| info.vendor.clone())
                        .unwrap_or_default(),
                    category: plugin_info
                        .map(|info| info.category.clone())
                        .unwrap_or_default(),
                    loaded: plugin_loaded,
                    enabled: node.enabled,
                    bypassed: node.bypassed,
                    has_editor,
                    editor_open: self
                        .rack
                        .plugin_editors
                        .get(node_id)
                        .is_some_and(|editor: &PluginEditor| editor.is_open()),
                    preferred_editor_size: self
                        .rack
                        .plugin_editors
                        .get(node_id)
                        .map(|editor: &PluginEditor| editor.preferred_size())
                        .unwrap_or((600, 400)),
                })
            })
            .collect()
    }
}
