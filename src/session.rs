use serde::{Deserialize, Serialize};

use crate::audio::node::{NodeId, SerializedGraph, SerializedNode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub sample_rate: f64,
    pub buffer_size: u32,
    #[serde(default)]
    pub preset: Preset,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<ChainSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph: Option<SerializedGraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub graph: SerializedGraph,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rack_order: Vec<NodeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSlot {
    pub plugin_path: String,
    pub plugin_name: String,
    pub enabled: bool,
    pub parameters: Vec<(usize, f32)>,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            graph: SerializedGraph::default(),
            rack_order: Vec::new(),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            sample_rate: 48000.0,
            buffer_size: 256,
            preset: Preset::default(),
            chain: Vec::new(),
            graph: None,
        }
    }
}

impl Preset {
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let preset: Preset = serde_json::from_str(&json)?;
        Ok(preset)
    }
}

impl Session {
    #[cfg(test)]
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let mut session: Session = serde_json::from_str(&json)?;
        session.promote_preset();
        Ok(session)
    }

    fn promote_preset(&mut self) {
        let has_preset_graph =
            !self.preset.graph.nodes.is_empty() || !self.preset.graph.connections.is_empty();

        if !has_preset_graph {
            if let Some(graph) = self.graph.take() {
                self.preset = Preset {
                    name: self.name.clone(),
                    graph,
                    rack_order: Vec::new(),
                };
            } else if !self.chain.is_empty() {
                self.preset = Preset {
                    name: self.name.clone(),
                    graph: Self::migrate_legacy_session(&self.chain),
                    rack_order: Vec::new(),
                };
                log::info!(
                    "Migrated {} legacy chain slots to preset graph format",
                    self.chain.len()
                );
            }
        }

        if self.preset.name.is_empty() {
            self.preset.name = self.name.clone();
        }
        if self.name.is_empty() {
            self.name = self.preset.name.clone();
        }

        self.graph = None;
        self.chain.clear();
    }

    fn migrate_legacy_session(chain: &[ChainSlot]) -> SerializedGraph {
        let mut nodes: Vec<SerializedNode> = Vec::new();
        let mut connections: Vec<crate::audio::node::Connection> = Vec::new();

        let input_id = crate::audio::node::NodeId(1);
        nodes.push(SerializedNode {
            id: input_id,
            node_type: crate::audio::node::NodeType::AudioInput,
            enabled: true,
            bypassed: false,
            position: (0.0, 0.0),
            parameters: Vec::new(),
            plugin_state: None,
            internal_state: crate::audio::node::NodeInternalState::None,
        });

        let output_id = crate::audio::node::NodeId(2);
        nodes.push(SerializedNode {
            id: output_id,
            node_type: crate::audio::node::NodeType::AudioOutput,
            enabled: true,
            bypassed: false,
            position: (0.0, 0.0),
            parameters: Vec::new(),
            plugin_state: None,
            internal_state: crate::audio::node::NodeInternalState::None,
        });

        let mut prev_id = input_id;
        let mut next_id_val: u64 = 3;

        for (i, slot) in chain.iter().enumerate() {
            let node_id = crate::audio::node::NodeId(next_id_val);
            let x = (i as f32 + 1.0) * 200.0;

            nodes.push(SerializedNode {
                id: node_id,
                node_type: crate::audio::node::NodeType::VstPlugin {
                    plugin_path: slot.plugin_path.clone(),
                    plugin_name: slot.plugin_name.clone(),
                },
                enabled: slot.enabled,
                bypassed: false,
                position: (x, 0.0),
                parameters: slot
                    .parameters
                    .iter()
                    .map(|(idx, val)| (*idx as u32, *val))
                    .collect(),
                plugin_state: None,
                internal_state: crate::audio::node::NodeInternalState::None,
            });

            connections.push(crate::audio::node::Connection {
                source_node: prev_id,
                source_port: crate::audio::node::PortId(0),
                target_node: node_id,
                target_port: crate::audio::node::PortId(0),
            });

            prev_id = node_id;
            next_id_val += 1;
        }

        connections.push(crate::audio::node::Connection {
            source_node: prev_id,
            source_port: crate::audio::node::PortId(0),
            target_node: output_id,
            target_port: crate::audio::node::PortId(0),
        });

        SerializedGraph { nodes, connections }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::node::{Connection, NodeId, NodeType, PortId};

    #[test]
    fn test_session_default_has_empty_preset() {
        let s = Session::default();
        assert!(s.preset.graph.nodes.is_empty());
        assert!(s.preset.graph.connections.is_empty());
        assert!(s.graph.is_none());
        assert!(s.chain.is_empty());
    }

    #[test]
    fn test_session_roundtrip() {
        let mut session = Session::default();
        session.name = "test_session".into();
        session.preset = Preset {
            name: "test_preset".into(),
            graph: SerializedGraph {
                nodes: vec![
                    SerializedNode {
                        id: NodeId(1),
                        node_type: NodeType::AudioInput,
                        enabled: true,
                        bypassed: false,
                        position: (0.0, 0.0),
                        ..Default::default()
                    },
                    SerializedNode {
                        id: NodeId(2),
                        node_type: NodeType::VstPlugin {
                            plugin_path: "C:/Plugins/Test.vst3".into(),
                            plugin_name: "Test".into(),
                        },
                        enabled: true,
                        bypassed: false,
                        position: (200.0, 0.0),
                        parameters: vec![(0, 0.25), (1, 0.75)],
                        plugin_state: Some(vec![1, 2, 3, 4]),
                        internal_state: Default::default(),
                    },
                    SerializedNode {
                        id: NodeId(3),
                        node_type: NodeType::AudioOutput,
                        enabled: true,
                        bypassed: false,
                        position: (400.0, 0.0),
                        ..Default::default()
                    },
                ],
                connections: vec![
                    Connection {
                        source_node: NodeId(1),
                        source_port: PortId(0),
                        target_node: NodeId(2),
                        target_port: PortId(0),
                    },
                    Connection {
                        source_node: NodeId(2),
                        source_port: PortId(0),
                        target_node: NodeId(3),
                        target_port: PortId(0),
                    },
                ],
            },
            rack_order: vec![NodeId(2)],
        };

        let dir = std::env::temp_dir().join("tonedock_test_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.tonedock.json");
        session.save_to_file(&path).unwrap();

        let loaded = Session::load_from_file(&path).unwrap();
        assert_eq!(loaded.name, "test_session");
        assert_eq!(loaded.preset.name, "test_preset");
        assert_eq!(loaded.preset.graph.nodes.len(), 3);
        assert_eq!(loaded.preset.graph.connections.len(), 2);
        assert_eq!(loaded.preset.rack_order, vec![NodeId(2)]);
        assert_eq!(
            loaded.preset.graph.nodes[1].plugin_state,
            Some(vec![1, 2, 3, 4])
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_legacy_graph_promoted_to_preset() {
        let mut session = Session::default();
        session.name = "legacy_graph".into();
        session.graph = Some(SerializedGraph {
            nodes: vec![
                SerializedNode {
                    id: NodeId(1),
                    node_type: NodeType::AudioInput,
                    ..Default::default()
                },
                SerializedNode {
                    id: NodeId(2),
                    node_type: NodeType::AudioOutput,
                    ..Default::default()
                },
            ],
            connections: vec![],
        });

        let dir = std::env::temp_dir().join("tonedock_test_graph_legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy_graph.tonedock.json");
        session.save_to_file(&path).unwrap();

        let loaded = Session::load_from_file(&path).unwrap();
        assert_eq!(loaded.preset.name, "legacy_graph");
        assert_eq!(loaded.preset.graph.nodes.len(), 2);
        assert!(loaded.graph.is_none());
        assert!(loaded.chain.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_legacy_migration() {
        let mut session = Session::default();
        session.name = "legacy".into();
        session.chain = vec![
            ChainSlot {
                plugin_path: "/path/to/amp.dll".into(),
                plugin_name: "Amp Sim".into(),
                enabled: true,
                parameters: vec![(0, 0.5), (1, 0.8)],
            },
            ChainSlot {
                plugin_path: "/path/to/reverb.dll".into(),
                plugin_name: "Reverb".into(),
                enabled: false,
                parameters: vec![],
            },
        ];

        let dir = std::env::temp_dir().join("tonedock_test_legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.tonedock.json");
        session.save_to_file(&path).unwrap();

        let loaded = Session::load_from_file(&path).unwrap();
        let g = &loaded.preset.graph;

        assert_eq!(loaded.preset.name, "legacy");
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.connections.len(), 3);

        assert!(g
            .nodes
            .iter()
            .any(|n| matches!(n.node_type, NodeType::AudioInput)));
        assert!(g
            .nodes
            .iter()
            .any(|n| matches!(n.node_type, NodeType::AudioOutput)));
        assert!(g.nodes.iter().any(|n| matches!(
            &n.node_type,
            NodeType::VstPlugin {
                plugin_path,
                plugin_name,
            } if plugin_path == "/path/to/amp.dll" && plugin_name == "Amp Sim"
        )));

        let amp_node = g
            .nodes
            .iter()
            .find(|n| {
                matches!(&n.node_type, NodeType::VstPlugin { plugin_name, .. } if plugin_name == "Amp Sim")
            })
            .unwrap();
        assert!(amp_node.enabled);
        assert_eq!(amp_node.parameters, vec![(0u32, 0.5f32), (1u32, 0.8f32)]);

        let reverb_node = g
            .nodes
            .iter()
            .find(|n| {
                matches!(&n.node_type, NodeType::VstPlugin { plugin_name, .. } if plugin_name == "Reverb")
            })
            .unwrap();
        assert!(!reverb_node.enabled);
        assert!(loaded.graph.is_none());
        assert!(loaded.chain.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_empty_chain_no_migration() {
        let mut session = Session::default();
        session.name = "empty".into();
        session.chain = vec![];

        let dir = std::env::temp_dir().join("tonedock_test_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.tonedock.json");
        session.save_to_file(&path).unwrap();

        let loaded = Session::load_from_file(&path).unwrap();
        assert!(loaded.preset.graph.nodes.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_legacy_migration_chain_order() {
        let mut session = Session::default();
        session.chain = vec![
            ChainSlot {
                plugin_path: "/a.dll".into(),
                plugin_name: "A".into(),
                enabled: true,
                parameters: vec![],
            },
            ChainSlot {
                plugin_path: "/b.dll".into(),
                plugin_name: "B".into(),
                enabled: true,
                parameters: vec![],
            },
        ];

        let g = Session::migrate_legacy_session(&session.chain);

        assert!(
            g.connections.iter().any(|c| {
                matches!(c.source_node, NodeId(1)) && matches!(c.target_node, NodeId(3))
            }),
            "Input -> first plugin"
        );
        assert!(
            g.connections.iter().any(|c| {
                matches!(c.source_node, NodeId(3)) && matches!(c.target_node, NodeId(4))
            }),
            "first plugin -> second plugin"
        );
        assert!(
            g.connections.iter().any(|c| {
                matches!(c.source_node, NodeId(4)) && matches!(c.target_node, NodeId(2))
            }),
            "second plugin -> Output"
        );
    }
}
