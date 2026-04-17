use std::collections::HashMap;
use std::sync::atomic::Ordering;

use crate::audio::node::{Connection, NodeId, NodeInternalState};

use super::{AudioGraph, BackingTrackBuffer, GraphNode, LooperBuffer};

impl AudioGraph {
    pub fn set_node_enabled(&mut self, id: NodeId, enabled: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.enabled = enabled;
        }
    }

    pub fn set_node_bypassed(&mut self, id: NodeId, bypassed: bool) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.bypassed = bypassed;
        }
    }

    pub fn set_node_position(&mut self, id: NodeId, x: f32, y: f32) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.position = (x, y);
        }
    }

    pub fn set_node_internal_state(&mut self, id: NodeId, state: NodeInternalState) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.internal_state = state;
        }
    }

    pub fn get_node(&self, id: NodeId) -> Option<&GraphNode> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut GraphNode> {
        self.nodes.get_mut(&id)
    }

    /// Returns the runtime/audio-thread view of a node. After commit_topology,
    /// heavy state (backing track / looper / recorder buffers, plugin_instance,
    /// and the live atomic counters) lives in `nodes_vec`, NOT in the `nodes`
    /// HashMap. Use this accessor whenever you need to read the post-commit
    /// state of a published graph.
    pub fn get_node_runtime(&self, id: NodeId) -> Option<&GraphNode> {
        let &idx = self.id_to_index.get(&id)?;
        self.nodes_vec.get(idx)
    }

    /// Locks the runtime plugin instance for a node and invokes `f` with a
    /// shared reference when a plugin is loaded. Returns `None` when the node
    /// is missing or has no plugin. Prefer this over manually chaining
    /// `get_node_runtime().plugin_instance.lock()` at call sites.
    pub fn with_plugin<R>(
        &self,
        id: NodeId,
        f: impl FnOnce(&crate::vst_host::plugin::LoadedPlugin) -> R,
    ) -> Option<R> {
        let node = self.get_node_runtime(id)?;
        let guard = node.plugin_instance.lock();
        guard.as_ref().map(f)
    }

    /// Moves heavy per-node runtime state (plugin instances, backing-track /
    /// looper / recorder buffers, and live atomics) from `prev`'s runtime view
    /// into this graph's runtime view.
    ///
    /// Safety/threading: This MUST only be called from the audio thread after
    /// it observes a graph change via `ArcSwap`. The audio thread is the sole
    /// reader/writer of `nodes_vec.*.buffers` (an `UnsafeCell`), so mutating
    /// both sides from a single thread avoids the data race that a UI-thread
    /// `transfer_runtime_*` would introduce.
    ///
    /// Skips the migration (and clears the flag) when `self.skip_runtime_migration`
    /// is set — used by preset loads that want the new graph to start with a
    /// clean runtime state.
    ///
    /// Policy: only migrates a slot when the destination is empty, so a freshly
    /// staged buffer (e.g. a newly loaded backing track) wins over the legacy
    /// buffer carried from the previous graph.
    pub fn migrate_runtime_state_from(&self, prev: &AudioGraph) {
        use std::sync::atomic::Ordering;

        if self
            .skip_runtime_migration
            .swap(false, Ordering::AcqRel)
        {
            return;
        }

        for node in &self.nodes_vec {
            let Some(&prev_idx) = prev.id_to_index.get(&node.id) else {
                continue;
            };
            let Some(prev_node) = prev.nodes_vec.get(prev_idx) else {
                continue;
            };
            if node.node_type != prev_node.node_type {
                continue;
            }

            {
                let mut dst = node.plugin_instance.lock();
                if dst.is_none() {
                    *dst = prev_node.plugin_instance.lock().take();
                }
            }

            let db = node.buffers_mut();
            let sb = prev_node.buffers_mut();
            if db.backing_track_buffer.is_none() {
                db.backing_track_buffer = sb.backing_track_buffer.take();
            }
            if db.looper_buffer.is_none() {
                db.looper_buffer = sb.looper_buffer.take();
            }
            if db.recorder_buffer.is_none() {
                db.recorder_buffer = sb.recorder_buffer.take();
            }
            db.metronome_phase = sb.metronome_phase;
            db.metronome_click_remaining = sb.metronome_click_remaining;
            db.backing_pre_roll_remaining = sb.backing_pre_roll_remaining;
            db.drum_phase = sb.drum_phase;
            db.drum_step = sb.drum_step;

            node.atomic_bt_position.store(
                prev_node.atomic_bt_position.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            node.atomic_bt_duration.store(
                prev_node.atomic_bt_duration.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            for i in 0..node.atomic_looper_lengths.len() {
                node.atomic_looper_lengths[i].store(
                    prev_node.atomic_looper_lengths[i].load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
            }
            node.atomic_recorder_has_data.store(
                prev_node.atomic_recorder_has_data.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
        }
    }

    #[allow(dead_code)]
    pub fn input_node_id(&self) -> Option<NodeId> {
        self.input_node_id
    }

    #[allow(dead_code)]
    pub fn output_node_id(&self) -> Option<NodeId> {
        self.output_node_id
    }

    pub fn connections(&self) -> &[Connection] {
        &self.connections
    }

    pub fn nodes(&self) -> &HashMap<NodeId, GraphNode> {
        &self.nodes
    }

    #[allow(dead_code)]
    pub fn nodes_mut(&mut self) -> &mut HashMap<NodeId, GraphNode> {
        &mut self.nodes
    }

    #[allow(dead_code)]
    pub fn process_order(&self) -> &[NodeId] {
        &self.process_order
    }

    #[allow(dead_code)]
    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
    }

    #[allow(dead_code)]
    pub fn set_max_frames(&mut self, frames: usize) {
        self.max_frames = frames;
        for node in self.nodes.values() {
            node.resize_buffers(frames);
        }
    }

    pub fn looper_loop_length(&self, node_id: NodeId) -> usize {
        // Read the live length from the runtime node (audio thread writes here).
        let runtime = self.get_node_runtime(node_id);
        let active_track = match runtime.map(|n| &n.internal_state) {
            Some(NodeInternalState::Looper(s)) => s.active_track as usize,
            _ => 0,
        };
        match runtime {
            Some(n) => n.atomic_looper_lengths[active_track].load(Ordering::Relaxed),
            None => 0,
        }
    }

    pub fn clear_looper(&mut self, node_id: NodeId) {
        if let Some(node) = self.nodes.get(&node_id) {
            let b = node.buffers_mut();
            if let Some(ref mut bufs) = b.looper_buffer {
                for buf in bufs.iter_mut() {
                    buf.clear();
                }
            }
        }
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.internal_state = NodeInternalState::Looper(crate::audio::node::LooperNodeState {
                enabled: false,
                recording: false,
                playing: false,
                overdubbing: false,
                cleared: false,
                fixed_length_beats: None,
                quantize_start: false,
                pre_fader: false,
                active_track: 0,
            });
        }
    }

    pub fn init_looper_buffer(&mut self, node_id: NodeId) {
        let sample_rate = self.sample_rate;
        let ch = if let Some(node) = self.nodes.get(&node_id) {
            node.output_ports
                .first()
                .map(|p| p.channels.channel_count())
                .unwrap_or(2)
        } else {
            return;
        };
        if let Some(node) = self.nodes.get(&node_id) {
            let b = node.buffers_mut();
            b.looper_buffer = Some(
                (0..4)
                    .map(|_| LooperBuffer::new(ch, sample_rate, 120.0))
                    .collect(),
            );
        }
    }

    pub fn set_backing_track_buffer(&self, node_id: NodeId, buffer: BackingTrackBuffer) {
        let Some(node) = self.nodes.get(&node_id) else {
            return;
        };
        let b = node.buffers_mut();
        b.backing_track_buffer = Some(buffer);
    }

    pub fn backing_track_duration_secs(&self, node_id: NodeId) -> f64 {
        let Some(node) = self.get_node_runtime(node_id) else {
            return 0.0;
        };
        f64::from_bits(node.atomic_bt_duration.load(Ordering::Relaxed))
    }

    pub fn backing_track_position_secs(&self, node_id: NodeId) -> f64 {
        let Some(node) = self.get_node_runtime(node_id) else {
            return 0.0;
        };
        f64::from_bits(node.atomic_bt_position.load(Ordering::Relaxed))
    }

    pub fn backing_track_seek(&self, node_id: NodeId, position_secs: f64) {
        // Seek targets the runtime buffer the audio thread is reading.
        // Falls back to the staging HashMap entry when called pre-commit.
        let node = self
            .get_node_runtime(node_id)
            .or_else(|| self.nodes.get(&node_id));
        let Some(node) = node else {
            return;
        };
        let b = node.buffers_mut();
        if let Some(ref mut buf) = b.backing_track_buffer {
            let pos = (position_secs * buf.sample_rate).clamp(0.0, buf.total_frames as f64);
            buf.playback_pos = pos;
            let clamped_secs = if buf.sample_rate > 0.0 {
                pos / buf.sample_rate
            } else {
                0.0
            };
            node.atomic_bt_position
                .store(clamped_secs.to_bits(), Ordering::Relaxed);
        }
    }

    #[allow(dead_code)]
    pub fn export_looper_samples(&self, node_id: NodeId) -> Option<Vec<Vec<f32>>> {
        let node = self.nodes.get(&node_id)?;
        let active_track = match &node.internal_state {
            NodeInternalState::Looper(s) => s.active_track as usize,
            _ => 0,
        };
        let b = node.buffers();
        match b.looper_buffer {
            Some(ref bufs) => bufs.get(active_track).and_then(|b| b.export_wav_samples()),
            None => None,
        }
    }

    #[allow(dead_code)]
    pub fn import_looper_samples(&self, node_id: NodeId, samples: &[Vec<f32>], count: usize) {
        let Some(node) = self.nodes.get(&node_id) else {
            return;
        };
        let b = node.buffers_mut();
        if let Some(ref mut bufs) = b.looper_buffer {
            if let Some(buf) = bufs.get_mut(0) {
                buf.import_samples(samples, count);
            }
        }
    }
}
