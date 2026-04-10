use crate::audio::node::{NodeId, NodeInternalState};

use super::AudioGraph;

impl AudioGraph {
    pub(super) fn process_metronome_node(&self, node_id: NodeId, num_frames: usize) {
        let (bpm, volume) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Metronome(state) => (state.bpm, state.volume),
                _ => (120.0, 0.5),
            }
        };

        let click_freq: f64 = 1000.0;
        let click_duration: usize = 480;
        let sample_rate = self.sample_rate;
        let samples_per_beat = sample_rate * 60.0 / bpm;

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut phase = node.metronome_phase.lock();
        let mut click_remaining = node.metronome_click_remaining.lock();

        if let Some(out_buf) = output_buffers.get_mut(0) {
            if out_buf.is_empty() {
                return;
            }
            let ch_count = out_buf.len();

            for frame in 0..num_frames {
                let sample = if *click_remaining > 0 {
                    let t = (click_duration - *click_remaining) as f64;
                    let val = (2.0 * std::f64::consts::PI * click_freq * t / sample_rate).sin()
                        * (*click_remaining as f64 / click_duration as f64);
                    *click_remaining -= 1;
                    val as f32 * volume
                } else {
                    0.0
                };

                for ch in 0..ch_count {
                    if frame < out_buf[ch].len() {
                        out_buf[ch][frame] = sample;
                    }
                }

                *phase += 1.0;
                if *phase >= samples_per_beat {
                    *phase -= samples_per_beat;
                    *click_remaining = click_duration;
                }
            }
        }
    }

    pub(super) fn process_looper_node(&self, node_id: NodeId, num_frames: usize) {
        let state: crate::audio::node::LooperNodeState = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Looper(s) => s.clone(),
                _ => return,
            }
        };

        if !state.enabled {
            return;
        }

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.get(0).and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();

        if state.recording {
            if let Some(ref input_buf) = input_data {
                let mut looper = node.looper_buffer.lock();
                if let Some(ref mut buf) = *looper {
                    buf.record(input_buf, num_frames);
                }
            }
        }

        if state.playing {
            let mut looper = node.looper_buffer.lock();
            if let Some(ref mut buf) = *looper {
                if let Some(out_buf) = output_buffers.get_mut(0) {
                    let ch_count = out_buf.len();
                    let mut temp_out = vec![vec![0.0f32; num_frames]; ch_count];
                    buf.read_and_advance(&mut temp_out, num_frames);
                    for ch in 0..ch_count {
                        let len = num_frames.min(out_buf[ch].len()).min(temp_out[ch].len());
                        for i in 0..len {
                            out_buf[ch][i] += temp_out[ch][i];
                        }
                    }
                }
            }
        } else if let Some(ref input_buf) = input_data {
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let ch_count = input_buf.len().min(out_buf.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input_buf[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_buf[ch][..len]);
                }
            }
        }

        if state.overdubbing {
            if let Some(ref input_buf) = input_data {
                let mut looper = node.looper_buffer.lock();
                if let Some(ref mut buf) = *looper {
                    buf.overdub(input_buf, num_frames);
                }
            }
        }
    }

    pub(super) fn process_vst_node(&self, node_id: NodeId, num_frames: usize) {
        let input_data: Vec<Vec<f32>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers
                .get(0)
                .and_then(|opt| opt.clone())
                .unwrap_or_default()
        };

        let num_ch = {
            let node = self.nodes.get(&node_id).unwrap();
            node.output_ports
                .first()
                .map(|p| p.channels.channel_count())
                .unwrap_or(0)
        };

        if num_ch == 0 {
            return;
        }

        let has_plugin = {
            let node = self.nodes.get(&node_id).unwrap();
            node.plugin_instance.lock().is_some()
        };

        if has_plugin {
            let mut temp_io: Vec<Vec<f32>> = vec![vec![0.0f32; num_frames]; num_ch];
            let ch_count = input_data.len().min(num_ch);
            for ch in 0..ch_count {
                let copy_len = num_frames.min(input_data[ch].len()).min(temp_io[ch].len());
                temp_io[ch][..copy_len].copy_from_slice(&input_data[ch][..copy_len]);
            }

            {
                let node = self.nodes.get(&node_id).unwrap();
                let mut plugin_instance = node.plugin_instance.lock();
                if let Some(ref mut plugin) = *plugin_instance {
                    let mut slices: Vec<&mut [f32]> =
                        temp_io.iter_mut().map(|v| &mut v[..]).collect();
                    plugin.process_in_place(&mut slices, num_frames as i32);
                }
            }

            let node = self.nodes.get(&node_id).unwrap();
            let mut output_buffers = node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let out_ch = out_buf.len().min(num_ch);
                for ch in 0..out_ch {
                    let len = num_frames.min(temp_io[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&temp_io[ch][..len]);
                }
            }
        } else {
            let node = self.nodes.get(&node_id).unwrap();
            let mut output_buffers = node.output_buffers.lock();
            if let Some(out_buf) = output_buffers.get_mut(0) {
                let out_ch = out_buf.len();
                let in_ch = input_data.len();
                let ch_count = in_ch.min(out_ch);
                for ch in 0..ch_count {
                    let len = num_frames.min(input_data[ch].len()).min(out_buf[ch].len());
                    out_buf[ch][..len].copy_from_slice(&input_data[ch][..len]);
                }
            }
        }
    }
}
