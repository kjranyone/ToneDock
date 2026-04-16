use std::sync::atomic::Ordering;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::audio::graph::AudioGraph;
use crate::audio::graph_command::GraphCommand;
use crate::audio::node::{NodeId, NodeInternalState, NodeType};

pub(super) fn apply_command(graph: &mut AudioGraph, cmd: GraphCommand) {
    match cmd {
        GraphCommand::AddNode(node_type) => {
            if let Ok(_id) = graph.add_node(node_type) {
                log::debug!("GraphCommand::AddNode processed");
            }
        }
        GraphCommand::RemoveNode(id) => {
            graph.remove_node(id);
            log::debug!("GraphCommand::RemoveNode({:?}) processed", id);
        }
        GraphCommand::SetNodeEnabled(id, enabled) => {
            graph.set_node_enabled(id, enabled);
        }
        GraphCommand::SetNodeBypassed(id, bypassed) => {
            graph.set_node_bypassed(id, bypassed);
        }
        GraphCommand::SetNodeState(id, state) => {
            if let Some(node) = graph.get_node_mut(id) {
                if let NodeInternalState::Looper(ref looper_state) = state {
                    if looper_state.cleared {
                        graph.clear_looper(id);
                        return;
                    }
                }
                node.internal_state = state;
            }
        }
        GraphCommand::SetNodePosition(id, x, y) => {
            graph.set_node_position(id, x, y);
        }
        GraphCommand::Connect(conn) => {
            if let Err(e) = graph.connect(conn) {
                log::warn!("GraphCommand::Connect failed: {}", e);
            }
        }
        GraphCommand::Disconnect { source, target } => {
            graph.disconnect(source, target);
        }
        GraphCommand::CommitTopology => {
            if let Err(e) = graph.commit_topology() {
                log::error!("GraphCommand::CommitTopology failed: {}", e);
            }
        }
    }
}

pub(super) fn prepare_runtime_graph(
    staging: &mut AudioGraph,
    runtime_config: Option<(f64, usize)>,
) {
    let Some((sample_rate, max_frames)) = runtime_config else {
        return;
    };

    staging.set_sample_rate(sample_rate);
    staging.set_max_frames(max_frames);

    let looper_ids: Vec<NodeId> = staging
        .nodes()
        .iter()
        .filter_map(|(&id, node)| matches!(node.node_type, NodeType::Looper).then_some(id))
        .collect();

    for looper_id in looper_ids {
        staging.init_looper_buffer(looper_id);
    }
}

fn transfer_runtime_plugins(
    current: &AudioGraph,
    staging: &mut AudioGraph,
    runtime_config: Option<(f64, usize)>,
) -> anyhow::Result<()> {
    if let Some((sample_rate, max_frames)) = runtime_config {
        for node in current.nodes().values() {
            let mut plugin_instance = node.plugin_instance.lock();
            if let Some(ref mut plugin) = *plugin_instance {
                plugin.setup_processing(sample_rate, max_frames as i32)?;
            }
        }
    }

    for (&id, source_node) in current.nodes() {
        let Some(target_node) = staging.get_node(id) else {
            continue;
        };

        let maybe_plugin = source_node.plugin_instance.lock().take();
        if let Some(plugin) = maybe_plugin {
            *target_node.plugin_instance.lock() = Some(plugin);
        }
    }

    Ok(())
}

pub(super) fn transfer_runtime_buffers(current: &AudioGraph, staging: &AudioGraph) {
    for (&id, source_node) in current.nodes() {
        let Some(target_node) = staging.get_node(id) else {
            continue;
        };

        if source_node.node_type != target_node.node_type {
            continue;
        }

        let sb = source_node.buffers();
        let tb = target_node.buffers_mut();

        if sb.looper_buffer.is_some() {
            tb.looper_buffer = sb.looper_buffer.clone();
        }
        if sb.backing_track_buffer.is_some() {
            tb.backing_track_buffer = sb.backing_track_buffer.clone();
        }
        if sb.recorder_buffer.is_some() {
            tb.recorder_buffer = sb.recorder_buffer.clone();
        }

        tb.metronome_phase = sb.metronome_phase;
        tb.metronome_click_remaining = sb.metronome_click_remaining;
        tb.backing_pre_roll_remaining = sb.backing_pre_roll_remaining;
        tb.drum_phase = sb.drum_phase;
        tb.drum_step = sb.drum_step;

        target_node.atomic_bt_position.store(
            source_node.atomic_bt_position.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        target_node.atomic_bt_duration.store(
            source_node.atomic_bt_duration.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        for (i, len) in source_node.atomic_looper_lengths.iter().enumerate() {
            target_node.atomic_looper_lengths[i]
                .store(len.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        target_node.atomic_recorder_has_data.store(
            source_node.atomic_recorder_has_data.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }
}

pub(super) fn commit_and_publish_graph(
    graph: &Arc<ArcSwap<AudioGraph>>,
    mut staging: AudioGraph,
    runtime_config: Option<(f64, usize)>,
) -> anyhow::Result<()> {
    prepare_runtime_graph(&mut staging, runtime_config);
    staging
        .commit_topology()
        .map_err(|e| anyhow::anyhow!("Topology commit failed: {}", e))?;

    let guard = graph.load();
    transfer_runtime_buffers(&guard, &staging);
    transfer_runtime_plugins(&guard, &mut staging, runtime_config)?;
    drop(guard);
    graph.store(Arc::new(staging));
    Ok(())
}
