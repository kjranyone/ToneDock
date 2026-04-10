use std::collections::HashMap;

use crate::audio::node::{NodeId, NodeType, PortId};

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

        let process_order = self.process_order.clone();
        for &node_id in &process_order {
            self.gather_inputs(node_id, num_frames);

            let (enabled, bypassed) = {
                let node = self.nodes.get(&node_id).unwrap();
                (node.enabled, node.bypassed)
            };

            if !enabled {
                continue;
            }

            if bypassed {
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
        let max_frames = self.max_frames;

        let incoming: Vec<(PortId, Vec<(NodeId, PortId)>)> = {
            let mut port_map: HashMap<PortId, Vec<(NodeId, PortId)>> = HashMap::new();
            for conn in &self.connections {
                if conn.target_node == node_id {
                    port_map
                        .entry(conn.target_port)
                        .or_default()
                        .push((conn.source_node, conn.source_port));
                }
            }
            port_map.into_iter().collect()
        };

        let input_ports_info: Vec<(usize, usize)> = {
            let node = self.nodes.get(&node_id).unwrap();
            node.input_ports
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.channels.channel_count()))
                .collect()
        };

        let mut new_input_buffers: Vec<Option<Vec<Vec<f32>>>> = vec![None; input_ports_info.len()];

        for (target_port_id, sources) in incoming {
            let port_idx = match input_ports_info.iter().position(|(i, _)| {
                let node = self.nodes.get(&node_id).unwrap();
                node.input_ports
                    .get(*i)
                    .map(|p| p.id == target_port_id)
                    .unwrap_or(false)
            }) {
                Some(idx) => input_ports_info[idx].0,
                None => continue,
            };

            let target_ch = input_ports_info
                .iter()
                .find(|(i, _)| *i == port_idx)
                .map(|(_, ch)| *ch)
                .unwrap_or(1);

            let mut mixed = vec![vec![0.0f32; max_frames]; target_ch];

            for (src_idx, (src_node_id, src_port_id)) in sources.iter().enumerate() {
                let src_buffers: Vec<Vec<f32>> = {
                    if let Some(src_node) = self.nodes.get(src_node_id) {
                        if let Some(pidx) = src_node
                            .output_ports
                            .iter()
                            .position(|p| p.id == *src_port_id)
                        {
                            let shared = src_node.shared_outputs.lock();
                            if let Some(ref sb) = shared.get(pidx).and_then(|opt| opt.as_ref()) {
                                sb.as_slice().to_vec()
                            } else {
                                drop(shared);
                                let output_buffers = src_node.output_buffers.lock();
                                if let Some(src_buf) = output_buffers.get(pidx) {
                                    src_buf.clone()
                                } else {
                                    continue;
                                }
                            }
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                };

                let src_ch_count = src_buffers.len();
                let target_ch_count = mixed.len();

                if src_idx == 0 {
                    if src_ch_count == target_ch_count {
                        for ch in 0..target_ch_count {
                            let copy_len =
                                num_frames.min(mixed[ch].len()).min(src_buffers[ch].len());
                            mixed[ch][..copy_len].copy_from_slice(&src_buffers[ch][..copy_len]);
                        }
                    } else if src_ch_count == 1 && target_ch_count == 2 {
                        let copy_len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        mixed[0][..copy_len].copy_from_slice(&src_buffers[0][..copy_len]);
                        mixed[1][..copy_len].copy_from_slice(&src_buffers[0][..copy_len]);
                    } else if src_ch_count == 2 && target_ch_count == 1 {
                        let copy_len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..copy_len {
                            mixed[0][i] = (src_buffers[0][i] + src_buffers[1][i]) * 0.5;
                        }
                    }
                } else {
                    if src_ch_count == target_ch_count {
                        for ch in 0..target_ch_count {
                            let len = num_frames.min(mixed[ch].len()).min(src_buffers[ch].len());
                            for i in 0..len {
                                mixed[ch][i] += src_buffers[ch][i];
                            }
                        }
                    } else if src_ch_count == 1 && target_ch_count == 2 {
                        let len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..len {
                            mixed[0][i] += src_buffers[0][i];
                            mixed[1][i] += src_buffers[0][i];
                        }
                    } else if src_ch_count == 2 && target_ch_count == 1 {
                        let len = num_frames.min(mixed[0].len()).min(src_buffers[0].len());
                        for i in 0..len {
                            mixed[0][i] += (src_buffers[0][i] + src_buffers[1][i]) * 0.5;
                        }
                    }
                }
            }

            if port_idx < new_input_buffers.len() {
                new_input_buffers[port_idx] = Some(mixed);
            }
        }

        let node = self.nodes.get(&node_id).unwrap();
        {
            let ports = &node.input_ports;
            for (i, port) in ports.iter().enumerate() {
                if i >= new_input_buffers.len() {
                    break;
                }
                if new_input_buffers[i].is_none() {
                    let ch_count = port.channels.channel_count();
                    new_input_buffers[i] = Some(vec![vec![0.0f32; max_frames]; ch_count]);
                }
            }
        }
        *node.input_buffers.lock() = new_input_buffers;
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
