use super::*;
use crate::audio::node::{ChannelConfig, Connection, NodeInternalState, NodeType, PortId};

#[test]
fn test_add_audio_input_output() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    assert_eq!(graph.input_node_id(), Some(input_id));
    assert_eq!(graph.output_node_id(), Some(output_id));
}

#[test]
fn test_singleton_violation() {
    let mut graph = AudioGraph::new(48000.0, 256);
    graph.add_node(NodeType::AudioInput).unwrap();
    let result = graph.add_node(NodeType::AudioInput);
    assert!(matches!(result, Err(GraphError::SingletonViolation)));
}

#[test]
fn test_connect_and_topology() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();
    let order = graph.process_order();

    assert_eq!(order.len(), 3);
    assert_eq!(order[0], input_id);
    assert_eq!(order[1], gain_id);
    assert_eq!(order[2], output_id);
}

#[test]
fn test_cycle_detection() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let a = graph.add_node(NodeType::Gain).unwrap();
    let b = graph.add_node(NodeType::Gain).unwrap();

    graph
        .connect(Connection {
            source_node: a,
            source_port: PortId(0),
            target_node: b,
            target_port: PortId(0),
        })
        .unwrap();

    let result = graph.connect(Connection {
        source_node: b,
        source_port: PortId(0),
        target_node: a,
        target_port: PortId(0),
    });

    assert!(matches!(result, Err(GraphError::CycleDetected)));
}

#[test]
fn test_remove_node_cleans_connections() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();

    assert_eq!(graph.connections().len(), 1);
    graph.remove_node(gain_id);
    assert!(graph.connections().is_empty());
}

#[test]
fn test_process_simple_chain() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();

    let input = vec![vec![0.5f32; 256]];
    let output = graph.process(&input, 256);

    assert_eq!(output.len(), 2);
    assert_eq!(output[0].len(), 256);
    assert_eq!(output[1].len(), 256);
}

#[test]
fn test_process_with_gain() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        let gain_node = graph.get_node_mut(gain_id).unwrap();
        gain_node.internal_state = NodeInternalState::Gain { value: 0.5 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        assert!((output[0][i] - 0.5).abs() < 0.001);
        assert!((output[1][i] - 0.5).abs() < 0.001);
    }
}

#[test]
fn test_pan_node() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let pan_id = graph.add_node(NodeType::Pan).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        let pan_node = graph.get_node_mut(pan_id).unwrap();
        pan_node.internal_state = NodeInternalState::Pan { value: 1.0 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: pan_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph
        .connect(Connection {
            source_node: pan_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..10 {
        let l = output[0][i];
        let r = output[1][i];
        assert!(r > l, "Full right pan: R({}) should be > L({})", r, l);
    }
}

#[test]
fn test_splitter_mixer_parallel() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
    let gain_a_id = graph.add_node(NodeType::Gain).unwrap();
    let gain_b_id = graph.add_node(NodeType::Gain).unwrap();
    let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        let node = graph.get_node_mut(gain_a_id).unwrap();
        node.internal_state = NodeInternalState::Gain { value: 0.5 };
    }
    {
        let node = graph.get_node_mut(gain_b_id).unwrap();
        node.internal_state = NodeInternalState::Gain { value: 0.3 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: splitter_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(0),
            target_node: gain_a_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(1),
            target_node: gain_b_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_a_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_b_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: mixer_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        let expected = 0.5 + 0.3;
        assert!(
            (output[0][i] - expected).abs() < 0.001,
            "Expected {} but got {} at frame {}",
            expected,
            output[0][i],
            i
        );
    }
}

#[test]
fn test_mono_stereo_auto_conversion_allowed() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let pan_id = graph.add_node(NodeType::Pan).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();

    let result = graph.connect(Connection {
        source_node: pan_id,
        source_port: PortId(0),
        target_node: gain_id,
        target_port: PortId(0),
    });

    assert!(
        result.is_ok(),
        "Stereo->Mono auto-conversion should be allowed"
    );
}

#[test]
fn test_disconnect() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    assert_eq!(graph.connections().len(), 1);
    graph.disconnect((input_id, PortId(0)), (gain_id, PortId(0)));
    assert!(graph.connections().is_empty());
}

#[test]
fn test_bypass_node() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        let gain_node = graph.get_node_mut(gain_id).unwrap();
        gain_node.internal_state = NodeInternalState::Gain { value: 0.0 };
        gain_node.bypassed = true;
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph.commit_topology().unwrap();

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        assert!(
            (output[0][i] - 1.0).abs() < 0.001,
            "Bypassed gain should pass through: got {}",
            output[0][i]
        );
    }
}

#[test]
fn test_disabled_node() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        graph.get_node_mut(gain_id).unwrap().enabled = false;
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph.commit_topology().unwrap();

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        assert!(
            (output[0][i]).abs() < 0.001,
            "Disabled node should output silence: got {}",
            output[0][i]
        );
    }
}

#[test]
fn test_set_node_position() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let id = graph.add_node(NodeType::Gain).unwrap();
    graph.set_node_position(id, 100.0, 200.0);
    let node = graph.get_node(id).unwrap();
    assert_eq!(node.position, (100.0, 200.0));
}

#[test]
fn test_set_node_enabled_bypassed() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let id = graph.add_node(NodeType::Gain).unwrap();
    assert!(graph.get_node(id).unwrap().enabled);
    assert!(!graph.get_node(id).unwrap().bypassed);

    graph.set_node_enabled(id, false);
    assert!(!graph.get_node(id).unwrap().enabled);

    graph.set_node_bypassed(id, true);
    assert!(graph.get_node(id).unwrap().bypassed);
}

#[test]
fn test_already_connected() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();

    let conn = Connection {
        source_node: input_id,
        source_port: PortId(0),
        target_node: gain_id,
        target_port: PortId(0),
    };
    graph.connect(conn.clone()).unwrap();
    let result = graph.connect(conn);
    assert!(matches!(result, Err(GraphError::AlreadyConnected)));
}

#[test]
fn test_reject_second_source_to_same_input_port() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let split_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: split_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: split_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();

    let result = graph.connect(Connection {
        source_node: split_id,
        source_port: PortId(1),
        target_node: gain_id,
        target_port: PortId(0),
    });
    assert!(matches!(result, Err(GraphError::AlreadyConnected)));
}

#[test]
fn test_vst_plugin_node_is_mono_in_stereo_out() {
    let node = GraphNode::new(
        NodeId(1),
        NodeType::VstPlugin {
            plugin_path: "test.vst3".into(),
            plugin_name: "Test".into(),
        },
        256,
    );
    assert_eq!(node.input_ports[0].channels.channel_count(), 1);
    assert_eq!(node.output_ports[0].channels.channel_count(), 2);
}

#[test]
fn test_connect_nonexistent_node() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let result = graph.connect(Connection {
        source_node: NodeId(999),
        source_port: PortId(0),
        target_node: NodeId(998),
        target_port: PortId(0),
    });
    assert!(matches!(result, Err(GraphError::NotFound)));
}

#[test]
fn test_wetdry_node() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
    let wetdry_id = graph.add_node(NodeType::WetDry).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        graph.get_node_mut(wetdry_id).unwrap().internal_state =
            NodeInternalState::WetDry { mix: 0.5 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: splitter_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(0),
            target_node: wetdry_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(1),
            target_node: wetdry_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: wetdry_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();
    let input = vec![vec![0.8f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        assert!(
            (output[0][i] - 0.8).abs() < 0.001,
            "Wet/Dry mix=0.5 should output ~0.8 at sample {}",
            i
        );
        assert!(
            (output[1][i] - 0.8).abs() < 0.001,
            "Wet/Dry mix=0.5 should output ~0.8 at sample {}",
            i
        );
    }
}

#[test]
fn test_wetdry_full_wet() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let splitter_id = graph.add_node(NodeType::Splitter { outputs: 2 }).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    {
        graph.get_node_mut(gain_id).unwrap().internal_state =
            NodeInternalState::Gain { value: 0.5 };
    }
    let wetdry_id = graph.add_node(NodeType::WetDry).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();
    {
        graph.get_node_mut(wetdry_id).unwrap().internal_state =
            NodeInternalState::WetDry { mix: 1.0 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: splitter_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(0),
            target_node: wetdry_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: splitter_id,
            source_port: PortId(1),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: wetdry_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: wetdry_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();
    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..256 {
        assert!(
            (output[0][i] - 0.5).abs() < 0.001,
            "Full wet should output gain*input=0.5 at sample {}",
            i
        );
    }
}

#[test]
fn test_send_return_bus() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let converter_id = graph
        .add_node(NodeType::ChannelConverter {
            target: ChannelConfig::Stereo,
        })
        .unwrap();
    let send_id = graph.add_node(NodeType::SendBus { bus_id: 1 }).unwrap();
    let return_id = graph.add_node(NodeType::ReturnBus { bus_id: 1 }).unwrap();
    let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        graph.get_node_mut(send_id).unwrap().internal_state =
            NodeInternalState::SendBus { send_level: 0.5 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: converter_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: converter_id,
            source_port: PortId(0),
            target_node: send_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: send_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: send_id,
            source_port: PortId(1),
            target_node: return_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: return_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: mixer_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();
    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    assert_eq!(output.len(), 2);
    for i in 0..10 {
        let mixed = 1.0 + 0.5;
        assert!(
            (output[0][i] - mixed).abs() < 0.01,
            "Output should be thru+send at sample {}: got {}",
            i,
            output[0][i]
        );
    }
}

#[test]
fn test_send_bus_zero_level() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let converter_id = graph
        .add_node(NodeType::ChannelConverter {
            target: ChannelConfig::Stereo,
        })
        .unwrap();
    let send_id = graph.add_node(NodeType::SendBus { bus_id: 1 }).unwrap();
    let return_id = graph.add_node(NodeType::ReturnBus { bus_id: 1 }).unwrap();
    let mixer_id = graph.add_node(NodeType::Mixer { inputs: 2 }).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    {
        graph.get_node_mut(send_id).unwrap().internal_state =
            NodeInternalState::SendBus { send_level: 0.0 };
    }

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: converter_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: converter_id,
            source_port: PortId(0),
            target_node: send_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: send_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: send_id,
            source_port: PortId(1),
            target_node: return_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: return_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: mixer_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.commit_topology().unwrap();
    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);

    for i in 0..10 {
        assert!(
            (output[0][i] - 1.0).abs() < 0.01,
            "Zero send should give only thru signal at sample {}: got {}",
            i,
            output[0][i]
        );
    }
}

#[test]
fn test_add_node_with_id() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let _ = graph.add_node(NodeType::AudioInput).unwrap();
    let _ = graph.add_node(NodeType::AudioOutput).unwrap();

    graph.add_node_with_id(NodeId(10), NodeType::Gain).unwrap();
    assert!(graph.get_node(NodeId(10)).is_some());
    assert!(matches!(
        graph.get_node(NodeId(10)).unwrap().node_type,
        NodeType::Gain
    ));
}

#[test]
fn test_add_node_with_id_updates_next_id() {
    let mut graph = AudioGraph::new(48000.0, 256);
    graph.add_node_with_id(NodeId(100), NodeType::Gain).unwrap();
    let next = graph.add_node(NodeType::Pan).unwrap();
    assert!(next.0 > 100, "next_node_id should be > 100, got {}", next.0);
}

#[test]
fn test_undo_remove_node_restore() {
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    graph.set_node_internal_state(gain_id, NodeInternalState::Gain { value: 1.5 });
    graph.set_node_position(gain_id, 100.0, 200.0);
    graph.commit_topology().unwrap();

    graph.remove_node(gain_id);
    assert!(graph.get_node(gain_id).is_none());
    assert_eq!(graph.connections().len(), 0);

    graph.add_node_with_id(gain_id, NodeType::Gain).unwrap();
    graph.set_node_position(gain_id, 100.0, 200.0);
    graph.set_node_internal_state(gain_id, NodeInternalState::Gain { value: 1.5 });
    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph.commit_topology().unwrap();

    assert!(graph.get_node(gain_id).is_some());
    let node = graph.get_node(gain_id).unwrap();
    assert_eq!(node.position, (100.0, 200.0));
    assert!(matches!(
        node.internal_state,
        NodeInternalState::Gain { value: 1.5 }
    ));
    assert_eq!(graph.connections().len(), 2);

    let input = vec![vec![1.0f32; 256]];
    let output = graph.process(&input, 256);
    for i in 0..10 {
        assert!(
            (output[0][i] - 1.5).abs() < 0.01,
            "Restored gain node should apply gain at sample {}: got {}",
            i,
            output[0][i]
        );
    }
}

#[test]
fn test_commit_topology_moves_backing_track_buffer_to_nodes_vec() {
    // Regression: load_backing_track_file writes the buffer into nodes (HashMap),
    // but the audio thread reads nodes_vec. commit_topology must move the buffer
    // forward so the audio thread can play it.
    let mut graph = AudioGraph::new(48000.0, 4);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let bt_id = graph.add_node(NodeType::BackingTrack).unwrap();
    let mixer_id = graph
        .add_node(NodeType::Mixer { inputs: 2 })
        .unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();

    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: bt_id,
            source_port: PortId(0),
            target_node: mixer_id,
            target_port: PortId(1),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: mixer_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();

    // Put a 4-frame buffer with all 0.25 samples into the BackingTrack node.
    let buffer = super::BackingTrackBuffer::new(vec![vec![0.25; 4], vec![0.25; 4]], 48000.0);
    graph.set_backing_track_buffer(bt_id, buffer);

    // Mark the node as playing so the processor will read the buffer.
    graph.set_node_internal_state(
        bt_id,
        NodeInternalState::BackingTrack(crate::audio::node::BackingTrackNodeState {
            playing: true,
            volume: 1.0,
            speed: 1.0,
            looping: false,
            file_loaded: true,
            loop_start: None,
            loop_end: None,
            pitch_semitones: 0.0,
            pre_roll_secs: 0.0,
            section_markers: vec![],
        }),
    );

    graph.commit_topology().unwrap();

    // The buffer must now live in the runtime node, not the HashMap entry.
    let runtime = graph.get_node_runtime(bt_id).expect("runtime node");
    let runtime_buf = &runtime.buffers().backing_track_buffer;
    assert!(runtime_buf.is_some(), "runtime nodes_vec must own the buffer");
    assert_eq!(runtime_buf.as_ref().unwrap().total_frames, 4);

    let hashmap_buf = &graph.get_node(bt_id).unwrap().buffers().backing_track_buffer;
    assert!(
        hashmap_buf.is_none(),
        "buffer must be moved out of the HashMap entry"
    );

    // Process: the audio output should reflect the backing track samples.
    let input = vec![vec![0.0f32; 4]];
    let output = graph.process(&input, 4);
    for (i, sample) in output[0].iter().enumerate().take(4) {
        assert!(
            (sample - 0.25).abs() < 1e-3,
            "frame {} should play backing track sample 0.25, got {}",
            i,
            sample
        );
    }
}

#[test]
fn test_clone_then_commit_rebuilds_runtime_tables() {
    // Regression: cloning an already-committed graph used to preserve
    // topology_dirty=false, so a follow-up commit_topology() returned early
    // without populating nodes_vec/compiled_connections_vec/*_node_idx.
    // The published clone then made the audio thread silent (and racy on
    // any code path that publishes without first applying a topology
    // command — e.g. load_backing_track_file, backing_track_seek, etc.).
    let mut graph = AudioGraph::new(48000.0, 256);
    let input_id = graph.add_node(NodeType::AudioInput).unwrap();
    let gain_id = graph.add_node(NodeType::Gain).unwrap();
    let output_id = graph.add_node(NodeType::AudioOutput).unwrap();
    graph
        .connect(Connection {
            source_node: input_id,
            source_port: PortId(0),
            target_node: gain_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph
        .connect(Connection {
            source_node: gain_id,
            source_port: PortId(0),
            target_node: output_id,
            target_port: PortId(0),
        })
        .unwrap();
    graph.commit_topology().unwrap();
    assert_eq!(graph.nodes_vec.len(), 3, "sanity: source has 3 nodes");

    let mut staging = graph.clone();
    assert!(
        staging.topology_dirty,
        "clone must mark topology dirty so commit_topology rebuilds runtime tables"
    );
    assert!(staging.nodes_vec.is_empty(), "clone must start with empty runtime tables");

    staging.commit_topology().unwrap();
    assert_eq!(
        staging.nodes_vec.len(),
        3,
        "commit_topology after clone must repopulate nodes_vec"
    );
    assert!(staging.input_node_idx.is_some());
    assert!(staging.output_node_idx.is_some());
    assert_eq!(staging.compiled_connections_vec.len(), 3);

    let input = vec![vec![0.5f32; 256]];
    let output = staging.process(&input, 256);
    assert!(
        (output[0][0] - 0.5).abs() < 1e-6,
        "cloned+committed graph must process audio (got {})",
        output[0][0]
    );
}
