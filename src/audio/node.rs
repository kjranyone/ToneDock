use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelConfig {
    Mono,
    Stereo,
    Custom(u16),
}

impl ChannelConfig {
    pub fn channel_count(&self) -> usize {
        match self {
            ChannelConfig::Mono => 1,
            ChannelConfig::Stereo => 2,
            ChannelConfig::Custom(n) => *n as usize,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    AudioInput,
    AudioOutput,
    VstPlugin {
        plugin_path: String,
        plugin_name: String,
    },
    Mixer {
        inputs: u16,
    },
    Splitter {
        outputs: u16,
    },
    Pan,
    ChannelConverter {
        target: ChannelConfig,
    },
    Metronome,
    Looper,
    Gain,
    WetDry,
    SendBus {
        bus_id: u32,
    },
    ReturnBus {
        bus_id: u32,
    },
}

impl NodeType {
    pub fn input_ports(&self) -> Vec<Port> {
        match self {
            NodeType::AudioInput => vec![],
            NodeType::AudioOutput => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::VstPlugin { .. } => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            }],
            NodeType::Mixer { inputs } => (0..*inputs)
                .map(|i| Port {
                    id: PortId(i as u32),
                    name: format!("in_{}", i),
                    direction: PortDirection::Input,
                    channels: ChannelConfig::Mono,
                })
                .collect(),
            NodeType::Splitter { .. } => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            }],
            NodeType::Pan => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            }],
            NodeType::ChannelConverter { .. } => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            }],
            NodeType::Metronome => vec![],
            NodeType::Looper => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::Gain => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Mono,
            }],
            NodeType::WetDry => vec![
                Port {
                    id: PortId(0),
                    name: "dry_in".into(),
                    direction: PortDirection::Input,
                    channels: ChannelConfig::Mono,
                },
                Port {
                    id: PortId(1),
                    name: "wet_in".into(),
                    direction: PortDirection::Input,
                    channels: ChannelConfig::Mono,
                },
            ],
            NodeType::SendBus { .. } => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::ReturnBus { .. } => vec![Port {
                id: PortId(0),
                name: "in".into(),
                direction: PortDirection::Input,
                channels: ChannelConfig::Stereo,
            }],
        }
    }

    pub fn output_ports(&self) -> Vec<Port> {
        match self {
            NodeType::AudioInput => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Mono,
            }],
            NodeType::AudioOutput => vec![],
            NodeType::VstPlugin { .. } => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Mono,
            }],
            NodeType::Mixer { .. } => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Mono,
            }],
            NodeType::Splitter { outputs } => (0..*outputs)
                .map(|i| Port {
                    id: PortId(i as u32),
                    name: format!("out_{}", i),
                    direction: PortDirection::Output,
                    channels: ChannelConfig::Mono,
                })
                .collect(),
            NodeType::Pan => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::ChannelConverter { target } => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: *target,
            }],
            NodeType::Metronome => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::Looper => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Stereo,
            }],
            NodeType::Gain => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Mono,
            }],
            NodeType::WetDry => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Mono,
            }],
            NodeType::SendBus { .. } => vec![
                Port {
                    id: PortId(0),
                    name: "thru".into(),
                    direction: PortDirection::Output,
                    channels: ChannelConfig::Stereo,
                },
                Port {
                    id: PortId(1),
                    name: "send".into(),
                    direction: PortDirection::Output,
                    channels: ChannelConfig::Stereo,
                },
            ],
            NodeType::ReturnBus { .. } => vec![Port {
                id: PortId(0),
                name: "out".into(),
                direction: PortDirection::Output,
                channels: ChannelConfig::Stereo,
            }],
        }
    }

    pub fn is_singleton(&self) -> bool {
        matches!(self, NodeType::AudioInput | NodeType::AudioOutput)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub id: PortId,
    pub name: String,
    pub direction: PortDirection,
    pub channels: ChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeInternalState {
    None,
    Metronome(MetronomeNodeState),
    Looper(LooperNodeState),
    Gain { value: f32 },
    Pan { value: f32 },
    WetDry { mix: f32 },
    SendBus { send_level: f32 },
}

impl Default for NodeInternalState {
    fn default() -> Self {
        NodeInternalState::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetronomeNodeState {
    pub bpm: f64,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LooperNodeState {
    pub enabled: bool,
    pub recording: bool,
    pub playing: bool,
    pub overdubbing: bool,
    pub cleared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub enabled: bool,
    pub bypassed: bool,
    pub position: (f32, f32),
    #[serde(default)]
    pub parameters: Vec<(u32, f32)>,
    #[serde(default)]
    pub internal_state: NodeInternalState,
}

impl Default for SerializedNode {
    fn default() -> Self {
        Self {
            id: NodeId(0),
            node_type: NodeType::AudioInput,
            enabled: true,
            bypassed: false,
            position: (0.0, 0.0),
            parameters: Vec::new(),
            internal_state: NodeInternalState::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraph {
    pub nodes: Vec<SerializedNode>,
    pub connections: Vec<Connection>,
}

impl Default for SerializedGraph {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            connections: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub source_node: NodeId,
    pub source_port: PortId,
    pub target_node: NodeId,
    pub target_port: PortId,
}
