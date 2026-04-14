use crate::audio::node::{NodeId, NodeType};

use super::AudioGraph;

impl AudioGraph {
    pub(super) fn process_internal(&self, input: &[Vec<f32>], num_frames: usize) {
        if self.topology_dirty {
            return;
        }

        for &node_id in &self.process_order {
            let node = self.nodes.get(&node_id).unwrap();
            node.clear_output_buffers();
        }

        if let Some(input_id) = self.input_node_id {
            let input_node = self.nodes.get(&input_id).unwrap();
            let mut output_buffers = input_node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let ch_count = out_buf.len().min(input.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(out_buf[ch].len()).min(input[ch].len());
                    out_buf[ch][..copy_len].copy_from_slice(&input[ch][..copy_len]);
                }
            }
        }

        for &node_id in &self.process_order {
            self.gather_inputs(node_id, num_frames);

            let node = self.nodes.get(&node_id).unwrap();
            if !node.enabled {
                continue;
            }

            if node.bypassed {
                self.bypass_node(node_id, num_frames);
                continue;
            }

            self.process_node(node_id, num_frames);
        }
    }

    pub fn process(&self, input: &[Vec<f32>], num_frames: usize) -> Vec<Vec<f32>> {
        self.process_internal(input, num_frames);

        let output_id = self.output_node_id;
        if let Some(output_id) = output_id {
            let output_node = self.nodes.get(&output_id).unwrap();
            let input_buffers = output_node.input_buffers.lock();
            if let Some(buf) = input_buffers.get(0).and_then(|opt| opt.as_ref()) {
                let ch_count = buf.len().min(2);
                let mut result = vec![vec![0.0f32; num_frames]; 2];
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(buf[ch].len());
                    result[ch][..copy_len].copy_from_slice(&buf[ch][..copy_len]);
                }
                if ch_count == 1 {
                    let (left, right) = result.split_at_mut(1);
                    right[0][..num_frames].copy_from_slice(&left[0][..num_frames]);
                }
                return result;
            }
        }

        vec![vec![0.0f32; num_frames]; 2]
    }

    #[allow(dead_code)]
    pub fn process_into(&self, input: &[Vec<f32>], output: &mut [Vec<f32>], num_frames: usize) {
        for ch in output.iter_mut() {
            let len = num_frames.min(ch.len());
            ch[..len].fill(0.0);
        }

        self.process_internal(input, num_frames);

        let output_id = self.output_node_id;
        if let Some(output_id) = output_id {
            let output_node = self.nodes.get(&output_id).unwrap();
            let input_buffers = output_node.input_buffers.lock();
            if let Some(buf) = input_buffers.get(0).and_then(|opt| opt.as_ref()) {
                let ch_count = buf.len().min(output.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(output[ch].len()).min(buf[ch].len());
                    output[ch][..copy_len].copy_from_slice(&buf[ch][..copy_len]);
                }
                if ch_count == 1 && output.len() >= 2 {
                    let copy_len = num_frames.min(output[0].len()).min(buf[0].len());
                    output[1][..copy_len].copy_from_slice(&buf[0][..copy_len]);
                }
            }
        }
    }

    pub(super) fn gather_inputs(&self, node_id: NodeId, num_frames: usize) {
        let Some(compiled) = self.compiled_connections.get(&node_id) else {
            // No incoming connections, ensure input buffers are silent
            let node = self.nodes.get(&node_id).unwrap();
            let mut input_buffers = node.input_buffers.lock();
            for opt_buf in input_buffers.iter_mut() {
                if let Some(buf) = opt_buf {
                    for ch in buf.iter_mut() {
                        ch.fill(0.0);
                    }
                }
            }
            return;
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut input_buffers = node.input_buffers.lock();

        // 1. Clear existing input buffers
        for opt_buf in input_buffers.iter_mut() {
            if let Some(buf) = opt_buf {
                for ch in buf.iter_mut() {
                    ch.fill(0.0);
                }
            }
        }

        // 2. Aggregate from sources
        for cc in compiled {
            let Some(src_node) = self.nodes.get(&cc.source_node) else {
                continue;
            };

            let src_buffers: Vec<Vec<f32>> = {
                let shared = src_node.shared_outputs.lock();
                if let Some(sb) = shared.get(cc.source_port_idx).and_then(|o| o.as_ref()) {
                    sb.as_slice().to_vec()
                } else {
                    drop(shared);
                    let ob = src_node.output_buffers.lock();
                    ob.get(cc.source_port_idx).cloned().unwrap_or_default()
                }
            };

            if src_buffers.is_empty() {
                continue;
            }

            let target_port_idx = cc.target_port_idx;
            let target_ch = src_buffers.len();
            if let Some(opt_buf) = input_buffers.get_mut(target_port_idx) {
                if opt_buf.is_none() {
                    *opt_buf = Some(vec![vec![0.0f32; num_frames]; target_ch]);
                }
            }

            if let Some(Some(target_buf)) = input_buffers.get_mut(target_port_idx) {
                let src_ch = src_buffers.len();
                let dst_ch = target_buf.len();

                if src_ch == dst_ch {
                    for ch in 0..src_ch {
                        let len = num_frames
                            .min(src_buffers[ch].len())
                            .min(target_buf[ch].len());
                        for i in 0..len {
                            target_buf[ch][i] += src_buffers[ch][i];
                        }
                    }
                } else if src_ch == 1 && dst_ch == 2 {
                    let len = num_frames
                        .min(src_buffers[0].len())
                        .min(target_buf[0].len());
                    for i in 0..len {
                        target_buf[0][i] += src_buffers[0][i];
                        target_buf[1][i] += src_buffers[0][i];
                    }
                } else if src_ch == 2 && dst_ch == 1 {
                    let len = num_frames
                        .min(src_buffers[0].len())
                        .min(target_buf[0].len());
                    for i in 0..len {
                        target_buf[0][i] += (src_buffers[0][i] + src_buffers[1][i]) * 0.5;
                    }
                }
            }
        }
    }

    pub(super) fn process_node(&self, node_id: NodeId, num_frames: usize) {
        let node_type: NodeType = {
            let node = self.nodes.get(&node_id).unwrap();
            node.node_type.clone()
        };

        match &node_type {
            NodeType::AudioInput | NodeType::AudioOutput => {}
            NodeType::Pan => self.process_pan_node(node_id, num_frames),
            NodeType::Gain => self.process_gain_node(node_id, num_frames),
            NodeType::Mixer { .. } => self.process_mixer_node(node_id, num_frames),
            NodeType::Splitter { .. } => self.process_splitter_node(node_id, num_frames),
            NodeType::ChannelConverter { .. } => self.process_converter_node(node_id, num_frames),
            NodeType::Metronome => self.process_metronome_node(node_id, num_frames),
            NodeType::Looper => self.process_looper_node(node_id, num_frames),
            NodeType::VstPlugin { .. } => self.process_vst_node(node_id, num_frames),
            NodeType::WetDry => self.process_wetdry_node(node_id, num_frames),
            NodeType::SendBus { .. } => self.process_send_bus_node(node_id, num_frames),
            NodeType::ReturnBus { .. } => self.process_return_bus_node(node_id, num_frames),
            NodeType::BackingTrack => self.process_backing_track_node(node_id, num_frames),
            NodeType::DrumMachine => self.process_drum_machine_node(node_id, num_frames),
            NodeType::Recorder => self.process_recorder_node(node_id, num_frames),
        }
    }

    pub(super) fn bypass_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.first().and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.first_mut() {
                let in_ch = input_buf.len();
                let out_ch = out_buf.len();

                if in_ch == out_ch {
                    for ch in 0..in_ch {
                        let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                        out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                    }
                } else if in_ch == 1 && out_ch == 2 {
                    let len = num_frames
                        .min(input_buf[0].len())
                        .min(out_buf[0].len())
                        .min(out_buf[1].len());
                    out_buf[0][..len].copy_from_slice(&input_buf[0][..len]);
                    out_buf[1][..len].copy_from_slice(&input_buf[0][..len]);
                } else if in_ch == 2 && out_ch == 1 {
                    let len = num_frames
                        .min(input_buf[0].len())
                        .min(input_buf[1].len())
                        .min(out_buf[0].len());
                    for i in 0..len {
                        out_buf[0][i] = (input_buf[0][i] + input_buf[1][i]) * 0.5;
                    }
                }
            }
        }
    }
}
