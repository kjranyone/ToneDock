use std::sync::Arc;

use super::ToneDockApp;
use crate::audio::node::{ChannelConfig, Connection, NodeInternalState, NodeType, PortId};

impl ToneDockApp {
    pub(crate) fn apply_template(&mut self, name: &str, base_pos: (f32, f32)) {
        match name {
            "wide_stereo_amp" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let pan_l_id = self.audio_engine.add_node_with_position(
                    NodeType::Pan,
                    base_pos.0 - 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    if guard.get_node(pan_l_id).is_some() {
                        let mut staging = (**guard).clone();
                        drop(guard);
                        if let Some(n) = staging.get_node_mut(pan_l_id) {
                            n.internal_state = NodeInternalState::Pan { value: -0.8 };
                        }
                        self.audio_engine.graph.store(Arc::new(staging));
                    }
                }
                let pan_r_id = self.audio_engine.add_node_with_position(
                    NodeType::Pan,
                    base_pos.0 + 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(pan_r_id) {
                        n.internal_state = NodeInternalState::Pan { value: 0.8 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 250.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: pan_l_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: pan_r_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: pan_l_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: pan_r_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = self.i18n.tr("template.wide_stereo").into();
            }
            "dry_wet_blend" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let wetdry_id = self.audio_engine.add_node_with_position(
                    NodeType::WetDry,
                    base_pos.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(wetdry_id) {
                        n.internal_state = NodeInternalState::WetDry { mix: 0.5 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: wetdry_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: wetdry_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = self.i18n.tr("template.dry_wet").into();
            }
            "mono_stereo_reverb" => {
                let converter_id = self.audio_engine.add_node_with_position(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Stereo,
                    },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: converter_id,
                    source_port: PortId(0),
                    target_node: self.audio_engine.master_mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = self.i18n.tr("template.mono_stereo_reverb").into();
            }
            "send_return_reverb" => {
                let send_id = self.audio_engine.add_node_with_position(
                    NodeType::SendBus { bus_id: 1 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let return_id = self.audio_engine.add_node_with_position(
                    NodeType::ReturnBus { bus_id: 1 },
                    base_pos.0 + 120.0,
                    base_pos.1 + 200.0,
                );
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 350.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: send_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: send_id,
                    source_port: PortId(1),
                    target_node: return_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: return_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = self.i18n.tr("template.send_return").into();
            }
            "parallel_chain" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let gain_a_id = self.audio_engine.add_node_with_position(
                    NodeType::Gain,
                    base_pos.0 - 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(gain_a_id) {
                        n.internal_state = NodeInternalState::Gain { value: 0.8 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let gain_b_id = self.audio_engine.add_node_with_position(
                    NodeType::Gain,
                    base_pos.0 + 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(gain_b_id) {
                        n.internal_state = NodeInternalState::Gain { value: 0.6 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 250.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: gain_a_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: gain_b_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: gain_a_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: gain_b_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = self.i18n.tr("template.parallel").into();
            }
            _ => {
                self.status_message = self.i18n.trf("template.unknown", &[("name", name)]);
            }
        }
    }
}
