use std::sync::Arc;

use crate::audio::graph::AudioGraph;
use crate::audio::node::{NodeId, NodeInternalState, NodeType, SerializedGraph};
use crate::vst_host::plugin::LoadedPlugin;
use crate::vst_host::scanner::PluginInfo;

use super::helpers::prepare_runtime_graph;
use super::AudioEngine;

pub(super) fn apply_serialized_parameters(plugin: &mut LoadedPlugin, parameters: &[(u32, f32)]) {
    for (index, value) in parameters {
        plugin.set_parameter(*index as usize, *value);
    }
}

impl AudioEngine {
    pub fn snapshot_serialized_graph(&self) -> SerializedGraph {
        let guard = self.graph.load();
        let mut node_ids: Vec<NodeId> = guard.nodes().keys().copied().collect();
        node_ids.sort();

        let mut nodes = Vec::with_capacity(node_ids.len());
        for node_id in node_ids {
            let Some(node) = guard.get_node(node_id) else {
                continue;
            };

            let (parameters, plugin_state) = if matches!(node.node_type, NodeType::VstPlugin { .. })
            {
                guard
                    .with_plugin(node_id, |plugin| {
                        let parameters = plugin
                            .parameter_info()
                            .iter()
                            .enumerate()
                            .map(|(index, _)| (index as u32, plugin.get_parameter(index)))
                            .collect();
                        (parameters, plugin.save_state())
                    })
                    .unwrap_or_else(|| (Vec::new(), None))
            } else {
                (Vec::new(), None)
            };

            nodes.push(crate::audio::node::SerializedNode {
                id: node_id,
                node_type: node.node_type.clone(),
                enabled: node.enabled,
                bypassed: node.bypassed,
                position: node.position,
                parameters,
                plugin_state,
                internal_state: node.internal_state.clone(),
            });
        }

        let mut connections = guard.connections().to_vec();
        connections.sort_by_key(|conn| {
            (
                conn.source_node.0,
                conn.source_port.0,
                conn.target_node.0,
                conn.target_port.0,
            )
        });

        SerializedGraph { nodes, connections }
    }

    fn instantiate_serialized_plugin(
        &self,
        info: &PluginInfo,
        plugin_state: Option<&[u8]>,
        parameters: &[(u32, f32)],
    ) -> anyhow::Result<LoadedPlugin> {
        let mut plugin = LoadedPlugin::load(info)?;
        plugin.setup_processing(self.sample_rate, self.buffer_size as i32)?;

        let restored_from_state = if let Some(state) = plugin_state {
            match plugin.restore_state(state) {
                Ok(()) => true,
                Err(err) => {
                    log::warn!(
                        "Failed to restore saved state for '{}' from '{}': {}",
                        info.name,
                        info.path.display(),
                        err
                    );
                    false
                }
            }
        } else {
            false
        };

        if !restored_from_state {
            apply_serialized_parameters(&mut plugin, parameters);
        }

        Ok(plugin)
    }

    pub fn load_serialized_graph(&mut self, data: &SerializedGraph) -> anyhow::Result<()> {
        let mut new_graph = AudioGraph::new(self.sample_rate, self.buffer_size as usize);

        for sn in &data.nodes {
            new_graph
                .add_node_with_id(sn.id, sn.node_type.clone())
                .map_err(|err| anyhow::anyhow!("Failed to add node {:?}: {}", sn.id, err))?;

            new_graph.set_node_enabled(sn.id, sn.enabled);
            new_graph.set_node_bypassed(sn.id, sn.bypassed);
            new_graph.set_node_position(sn.id, sn.position.0, sn.position.1);
            if !matches!(sn.internal_state, NodeInternalState::None) {
                new_graph.set_node_internal_state(sn.id, sn.internal_state.clone());
            }
        }

        for conn in &data.connections {
            new_graph
                .connect(conn.clone())
                .map_err(|err| anyhow::anyhow!("Failed to connect {:?}: {}", conn, err))?;
        }

        // Instantiate and place plugins INTO new_graph.nodes BEFORE commit_topology;
        // commit_topology will then move them into new_graph.nodes_vec where the
        // audio thread reads them.
        for sn in &data.nodes {
            let NodeType::VstPlugin {
                plugin_path,
                plugin_name,
            } = &sn.node_type
            else {
                continue;
            };

            let info = PluginInfo {
                path: std::path::PathBuf::from(plugin_path),
                name: plugin_name.clone(),
                category: String::new(),
                vendor: String::new(),
            };

            match self.instantiate_serialized_plugin(
                &info,
                sn.plugin_state.as_deref(),
                &sn.parameters,
            ) {
                Ok(plugin) => {
                    if let Some(node) = new_graph.get_node_mut(sn.id) {
                        *node.plugin_instance.get_mut() = Some(plugin);
                    }
                }
                Err(err) => {
                    log::error!(
                        "Failed to restore VST '{}' from '{}': {}",
                        plugin_name,
                        plugin_path,
                        err
                    );
                }
            }
        }

        prepare_runtime_graph(
            &mut new_graph,
            Some((self.sample_rate, self.buffer_size as usize)),
        );
        new_graph
            .commit_topology()
            .map_err(|err| anyhow::anyhow!("Topology commit failed: {}", err))?;

        // Preset loads start the audio thread with a clean runtime state —
        // no carryover of the previous graph's looper / backing track /
        // recorder buffers. Flagging this on the staging graph makes the
        // audio thread skip its next `migrate_runtime_state_from` call.
        new_graph
            .skip_runtime_migration
            .store(true, std::sync::atomic::Ordering::Release);

        let metronome_node_id = Self::find_node_id_in_graph(&new_graph, NodeType::Metronome);
        let looper_node_id = Self::find_node_id_in_graph(&new_graph, NodeType::Looper);
        self.metronome_node_id = metronome_node_id;
        self.looper_node_id = looper_node_id;

        self.graph.store(Arc::new(new_graph));
        log::info!(
            "Loaded serialized graph: {} nodes, {} connections",
            data.nodes.len(),
            data.connections.len()
        );
        Ok(())
    }

    pub fn load_vst_plugin_to_node(
        &self,
        node_id: NodeId,
        info: &PluginInfo,
    ) -> anyhow::Result<()> {
        let plugin = self.instantiate_serialized_plugin(info, None, &[])?;

        // Safe: plugin_instance is Mutex-protected. The audio thread holds the lock during
        // process(), so this blocks until processing is done. The swap is atomic from the
        // audio thread's perspective (it sees either old or new plugin, never both).
        {
            let guard = self.graph.load();
            if let Some(node) = guard.get_node_runtime(node_id) {
                *node.plugin_instance.lock() = Some(plugin);
            } else {
                return Err(anyhow::anyhow!("Node {:?} not found in graph", node_id));
            }
        }

        log::info!("VST plugin '{}' loaded into node {:?}", info.name, node_id);
        Ok(())
    }

    pub(super) fn find_node_id_in_graph(graph: &AudioGraph, target: NodeType) -> Option<NodeId> {
        for (&id, node) in graph.nodes() {
            if std::mem::discriminant(&node.node_type) == std::mem::discriminant(&target) {
                return Some(id);
            }
        }
        None
    }

    pub fn set_vst_node_parameter(&self, node_id: NodeId, param_index: usize, value: f32) {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node_runtime(node_id) {
            let mut plugin_instance = node.plugin_instance.lock();
            if let Some(ref mut plugin) = *plugin_instance {
                plugin.set_parameter(param_index, value);
            }
        }
    }

    pub fn get_vst_node_parameters(&self, node_id: NodeId) -> Vec<crate::audio::chain::ParamInfo> {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node_runtime(node_id) {
            let plugin_instance = node.plugin_instance.lock();
            if let Some(ref plugin) = *plugin_instance {
                return plugin.parameter_info();
            }
        }
        Vec::new()
    }

    pub fn get_vst_node_parameter_value(&self, node_id: NodeId, param_index: usize) -> f32 {
        let guard = self.graph.load();
        if let Some(node) = guard.get_node_runtime(node_id) {
            let plugin_instance = node.plugin_instance.lock();
            if let Some(ref plugin) = *plugin_instance {
                return plugin.get_parameter(param_index);
            }
        }
        0.0
    }
}
