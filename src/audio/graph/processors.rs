use crate::audio::node::{NodeId, NodeInternalState};

use super::{AudioGraph, SharedBuffer};

impl AudioGraph {
    pub(super) fn process_pan_node(&self, node_id: NodeId, num_frames: usize) {
        let pan_value = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Pan { value } => *value,
                _ => 0.0,
            }
        };

        let angle = (pan_value + 1.0) * std::f32::consts::FRAC_PI_4;
        let gain_l = angle.cos();
        let gain_r = angle.sin();

        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();
        let input_data = b.input_buffers.get(0).and_then(|opt| opt.as_ref());
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = b.output_buffers.get_mut(0) {
                if out_buf.len() >= 2 && !input_buf.is_empty() {
                    let copy_len = num_frames
                        .min(input_buf[0].len())
                        .min(out_buf[0].len())
                        .min(out_buf[1].len());
                    for i in 0..copy_len {
                        out_buf[0][i] = input_buf[0][i] * gain_l;
                        out_buf[1][i] = input_buf[0][i] * gain_r;
                    }
                }
            }
        }
    }

    pub(super) fn process_gain_node(&self, node_id: NodeId, num_frames: usize) {
        let gain_value = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Gain { value } => *value,
                _ => 1.0,
            }
        };

        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();
        let input_data = b.input_buffers.get(0).and_then(|opt| opt.as_ref());
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = b.output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let copy_len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    for i in 0..copy_len {
                        out_buf[ch][i] = input_buf[ch][i] * gain_value;
                    }
                }
            }
        }
    }

    pub(super) fn process_mixer_node(&self, node_id: NodeId, num_frames: usize) {
        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();
        if let Some(out_buf) = b.output_buffers.get_mut(0) {
            for input_buf_opt in b.input_buffers.iter() {
                if let Some(input_buf) = input_buf_opt {
                    let ch_count = input_buf.len().min(out_buf.len());
                    for ch in 0..ch_count {
                        let len = num_frames.min(out_buf[ch].len()).min(input_buf[ch].len());
                        for i in 0..len {
                            out_buf[ch][i] += input_buf[ch][i];
                        }
                    }
                }
            }
        }
    }

    pub(super) fn process_splitter_node(&self, node_id: NodeId, num_frames: usize) {
        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();

        let Some(input_data) = b.input_buffers.get(0).and_then(|opt| opt.as_ref()) else {
            return;
        };

        let num_outputs = b.output_buffers.len();
        if num_outputs == 0 {
            return;
        }

        {
            let first_buf = &mut b.output_buffers[0];
            let ch_count = input_data.len().min(first_buf.len());
            for ch in 0..ch_count {
                let copy_len = num_frames
                    .min(input_data[ch].len())
                    .min(first_buf[ch].len());
                first_buf[ch][..copy_len].copy_from_slice(&input_data[ch][..copy_len]);
            }
        }

        let shared = SharedBuffer::from_vec(b.output_buffers[0].clone());
        for i in 1..num_outputs {
            b.shared_outputs[i] = Some(shared.clone());
        }
    }

    pub(super) fn process_converter_node(&self, node_id: NodeId, num_frames: usize) {
        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();

        let Some(input_data) = b.input_buffers.get(0).and_then(|opt| opt.as_ref()) else {
            return;
        };

        if let Some(out_buf) = b.output_buffers.get_mut(0) {
            let in_ch = input_data.len();
            let out_ch = out_buf.len();

            if in_ch == 1 && out_ch == 2 {
                let len = num_frames
                    .min(input_data[0].len())
                    .min(out_buf[0].len())
                    .min(out_buf[1].len());
                out_buf[0][..len].copy_from_slice(&input_data[0][..len]);
                out_buf[1][..len].copy_from_slice(&input_data[0][..len]);
            } else if in_ch == 2 && out_ch == 1 {
                let len = num_frames
                    .min(input_data[0].len())
                    .min(input_data[1].len())
                    .min(out_buf[0].len());
                for i in 0..len {
                    out_buf[0][i] = (input_data[0][i] + input_data[1][i]) * 0.5;
                }
            } else {
                let ch_count = in_ch.min(out_ch);
                for ch in 0..ch_count {
                    let len = num_frames.min(input_data[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_data[ch][..len]);
                }
            }
        }
    }

    pub(super) fn process_wetdry_node(&self, node_id: NodeId, num_frames: usize) {
        let mix = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::WetDry { mix } => *mix,
                _ => 0.5,
            }
        };

        let inv_mix = 1.0 - mix;
        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();

        if let Some(out_buf) = b.output_buffers.get_mut(0) {
            let ch_count = out_buf.len();
            for ch in 0..ch_count {
                let len = num_frames.min(out_buf[ch].len());
                let dry_ch = b
                    .input_buffers
                    .get(0)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|v| v.get(ch));
                let wet_ch = b
                    .input_buffers
                    .get(1)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|v| v.get(ch));
                match (dry_ch, wet_ch) {
                    (Some(dc), Some(wc)) => {
                        let l = len.min(dc.len()).min(wc.len());
                        for i in 0..l {
                            out_buf[ch][i] = dc[i] * inv_mix + wc[i] * mix;
                        }
                    }
                    (Some(dc), None) => {
                        let l = len.min(dc.len());
                        for i in 0..l {
                            out_buf[ch][i] = dc[i] * inv_mix;
                        }
                    }
                    (None, Some(wc)) => {
                        let l = len.min(wc.len());
                        for i in 0..l {
                            out_buf[ch][i] = wc[i] * mix;
                        }
                    }
                    (None, None) => {}
                }
            }
        }
    }

    pub(super) fn process_send_bus_node(&self, node_id: NodeId, num_frames: usize) {
        let send_level = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::SendBus { send_level } => *send_level,
                _ => 1.0,
            }
        };

        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();
        let input_data = b.input_buffers.get(0).and_then(|opt| opt.as_ref());

        if let Some(ref input_buf) = input_data {
            if let Some(thru_buf) = b.output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(thru_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(thru_buf[ch].len());
                    thru_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
            if let Some(send_buf) = b.output_buffers.get_mut(1) {
                let ch_count = input_buf.len().min(send_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(send_buf[ch].len());
                    for i in 0..len {
                        send_buf[ch][i] = input_buf[ch][i] * send_level;
                    }
                }
            }
        }
    }

    pub(super) fn process_return_bus_node(&self, node_id: NodeId, num_frames: usize) {
        let node = self.nodes.get(&node_id).unwrap();
        let b = node.buffers_mut();
        let input_data = b.input_buffers.get(0).and_then(|opt| opt.as_ref());
        if let Some(out_buf) = b.output_buffers.get_mut(0) {
            if let Some(ref input_buf) = input_data {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
        }
    }
}
