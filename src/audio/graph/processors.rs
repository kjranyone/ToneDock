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

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
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

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
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
        let input_buffers_data: Vec<Option<Vec<Vec<f32>>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            node.input_buffers.lock().clone()
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
            for ch in 0..out_buf.len() {
                for i in 0..num_frames.min(out_buf[ch].len()) {
                    out_buf[ch][i] = 0.0;
                }
            }

            for input_buf_opt in &input_buffers_data {
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
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut shared_outputs = node.shared_outputs.lock();
        let Some(input_data) = input_data else {
            return;
        };

        let num_outputs = output_buffers.len();
        if num_outputs == 0 {
            return;
        }

        {
            let first_buf = &mut output_buffers[0];
            let ch_count = input_data.len().min(first_buf.len());
            for ch in 0..ch_count {
                let copy_len = num_frames
                    .min(input_data[ch].len())
                    .min(first_buf[ch].len());
                first_buf[ch][..copy_len].copy_from_slice(&input_data[ch][..copy_len]);
            }
        }

        let shared = SharedBuffer::from_vec(output_buffers[0].clone());
        for i in 1..num_outputs {
            shared_outputs[i] = Some(shared.clone());
        }
    }

    pub(super) fn process_converter_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let Some(input_data) = input_data else {
            return;
        };

        if let Some(out_buf) = output_buffers.get_mut(0) {
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

        let dry_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let wet_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(1).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
            let ch_count = out_buf.len();
            for ch in 0..ch_count {
                let len = num_frames.min(out_buf[ch].len());
                for i in 0..len {
                    let dry = dry_data
                        .as_ref()
                        .and_then(|d| d.get(ch).map(|c| c.get(i).copied().unwrap_or(0.0)))
                        .unwrap_or(0.0);
                    let wet = wet_data
                        .as_ref()
                        .and_then(|w| w.get(ch).map(|c| c.get(i).copied().unwrap_or(0.0)))
                        .unwrap_or(0.0);
                    out_buf[ch][i] = dry * (1.0 - mix) + wet * mix;
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

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();

        if let Some(ref input_buf) = input_data {
            if let Some(thru_buf) = output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(thru_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(thru_buf[ch].len());
                    thru_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
            if let Some(send_buf) = output_buffers.get_mut(1) {
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
        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        if let Some(out_buf) = output_buffers.get_mut(0) {
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
