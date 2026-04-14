use crate::audio::node::{NodeId, NodeInternalState};

use super::AudioGraph;

const DRUM_KICK_FREQ: f64 = 60.0;
const DRUM_KICK_DECAY: usize = 2400;
const DRUM_SNARE_FREQ: f64 = 200.0;
const DRUM_SNARE_DECAY: usize = 1200;
const DRUM_HH_DECAY: usize = 400;
const DRUM_STEPS: u8 = 16;

const PATTERN_ROCK: [u8; 16] = [1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2, 0, 4, 0];
const PATTERN_BLUES: [u8; 16] = [1, 0, 0, 2, 0, 0, 1, 0, 0, 2, 0, 0, 1, 0, 2, 4];
const PATTERN_METAL: [u8; 16] = [1, 0, 2, 0, 1, 0, 2, 4, 1, 0, 2, 0, 1, 4, 2, 4];
const PATTERN_FUNK: [u8; 16] = [1, 0, 0, 1, 0, 0, 2, 0, 1, 0, 4, 0, 2, 0, 0, 0];
const PATTERN_JAZZ: [u8; 16] = [1, 0, 0, 4, 2, 0, 4, 0, 1, 0, 0, 4, 2, 0, 4, 0];

fn drum_pattern(idx: u8) -> &'static [u8; 16] {
    match idx % 5 {
        0 => &PATTERN_ROCK,
        1 => &PATTERN_BLUES,
        2 => &PATTERN_METAL,
        3 => &PATTERN_FUNK,
        _ => &PATTERN_JAZZ,
    }
}

fn cubic_interp(v0: f32, v1: f32, v2: f32, v3: f32, frac: f32) -> f32 {
    let a = -0.5 * v0 + 1.5 * v1 - 1.5 * v2 + 0.5 * v3;
    let b = v0 - 2.5 * v1 + 2.0 * v2 - 0.5 * v3;
    let c = -0.5 * v0 + 0.5 * v2;
    let d = v1;
    a * frac * frac * frac + b * frac * frac + c * frac + d
}

impl AudioGraph {
    pub(super) fn process_metronome_node(&self, node_id: NodeId, num_frames: usize) {
        let (bpm, volume, count_in_beats, count_in_active) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Metronome(state) => (
                    state.bpm,
                    state.volume,
                    state.count_in_beats,
                    state.count_in_active,
                ),
                _ => (120.0, 0.5, 0, false),
            }
        };

        let click_freq: f64 = 1000.0;
        let accent_freq: f64 = 1500.0;
        let click_duration: usize = 480;
        let sample_rate = self.sample_rate;
        let samples_per_beat = sample_rate * 60.0 / bpm;

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut phase = node.metronome_phase.lock();
        let mut click_remaining = node.metronome_click_remaining.lock();

        let total_beats = if count_in_active && count_in_beats > 0 {
            count_in_beats
        } else {
            0
        };
        let current_beat = (*phase / samples_per_beat).floor() as u32;

        if let Some(out_buf) = output_buffers.get_mut(0) {
            if out_buf.is_empty() {
                return;
            }
            let ch_count = out_buf.len();

            for frame in 0..num_frames {
                let sample = if *click_remaining > 0 {
                    let t = (click_duration - *click_remaining) as f64;
                    let freq = if total_beats > 0 && current_beat == 0 {
                        accent_freq
                    } else {
                        click_freq
                    };
                    let val = (2.0 * std::f64::consts::PI * freq * t / sample_rate).sin()
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
        let (state, active_track): (crate::audio::node::LooperNodeState, u8) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Looper(s) => (s.clone(), s.active_track),
                _ => return,
            }
        };

        if !state.enabled {
            return;
        }

        let fixed_length_samples = state.fixed_length_beats.and_then(|beats| {
            let samples_per_beat = self.sample_rate * 60.0 / 120.0;
            Some((beats as f64 * samples_per_beat) as usize)
        });

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
                if let Some(ref mut bufs) = *looper {
                    if let Some(buf) = bufs.get_mut(active_track as usize) {
                        buf.record(input_buf, num_frames);
                        if let Some(max_samples) = fixed_length_samples {
                            if buf.len >= max_samples {
                                buf.len = max_samples;
                            }
                        }
                    }
                }
            }
        }

        if state.playing {
            let mut looper = node.looper_buffer.lock();
            if let Some(ref mut bufs) = *looper {
                if let Some(buf) = bufs.get_mut(active_track as usize) {
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
                if let Some(ref mut bufs) = *looper {
                    if let Some(buf) = bufs.get_mut(active_track as usize) {
                        buf.overdub(input_buf, num_frames);
                    }
                }
            }
        }
    }

    pub(super) fn process_backing_track_node(&self, node_id: NodeId, num_frames: usize) {
        let (playing, volume, speed, pitch_semitones, looping, loop_start, loop_end, pre_roll_secs) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::BackingTrack(state) => (
                    state.playing,
                    state.volume,
                    state.speed,
                    state.pitch_semitones,
                    state.looping,
                    state.loop_start,
                    state.loop_end,
                    state.pre_roll_secs,
                ),
                _ => (false, 1.0, 1.0, 0.0, true, None, None, 0.0),
            }
        };

        if !playing {
            return;
        }

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut bt_buf = node.backing_track_buffer.lock();

        let Some(ref mut buf) = *bt_buf else {
            return;
        };
        if buf.total_frames == 0 {
            return;
        }

        let pre_roll_frames = (pre_roll_secs * self.sample_rate) as usize;
        if pre_roll_frames > 0 {
            let node = self.nodes.get(&node_id).unwrap();
            let mut pr = node.backing_pre_roll_remaining.lock();
            if *pr == 0 {
                *pr = pre_roll_frames;
            }
            if *pr > 0 {
                let silent = num_frames.min(*pr);
                *pr -= silent;
                drop(pr);
                if let Some(out_buf) = output_buffers.get_mut(0) {
                    for ch in out_buf.iter_mut() {
                        for f in ch.iter_mut().take(silent) {
                            *f = 0.0;
                        }
                    }
                }
                return;
            }
        }

        let Some(out_buf) = output_buffers.get_mut(0) else {
            return;
        };

        let pitch_mult = 2.0_f64.powf(pitch_semitones as f64 / 12.0);
        let ratio = if buf.sample_rate > 0.0 {
            buf.sample_rate / self.sample_rate * speed as f64 * pitch_mult
        } else {
            speed as f64 * pitch_mult
        };

        let sr = buf.sample_rate;
        let ab_start = loop_start.map(|s| (s * sr) as usize);
        let ab_end = loop_end.map(|e| (e * sr) as usize);

        let out_ch = out_buf.len();
        let src_ch = buf.channels;

        for frame in 0..num_frames {
            let pos = buf.playback_pos;
            let pos_floor = pos.floor() as usize;
            let frac = (pos - pos_floor as f64) as f32;

            if let (Some(start), Some(end)) = (ab_start, ab_end) {
                if end > start && pos_floor >= end {
                    if looping {
                        buf.playback_pos = start as f64 + (pos - end as f64);
                        continue;
                    } else {
                        for ch in 0..out_ch {
                            for f in &mut out_buf[ch][frame..num_frames] {
                                *f = 0.0;
                            }
                        }
                        break;
                    }
                }
            } else if pos_floor >= buf.total_frames {
                if looping {
                    buf.playback_pos = pos - buf.total_frames as f64;
                    continue;
                } else {
                    for ch in 0..out_ch {
                        for f in &mut out_buf[ch][frame..num_frames] {
                            *f = 0.0;
                        }
                    }
                    break;
                }
            }

            // Cubic Hermite Interpolation (4-point)
            let s1 = pos_floor;
            let (range_start, range_end) = if let (Some(s), Some(e)) = (ab_start, ab_end) {
                (s, e)
            } else {
                (0, buf.total_frames)
            };

            let s0 = if s1 > range_start {
                s1 - 1
            } else if looping {
                range_end.saturating_sub(1)
            } else {
                s1
            };
            let s2 = if s1 + 1 < range_end {
                s1 + 1
            } else if looping {
                range_start
            } else {
                s1
            };
            let s3 = if s2 + 1 < range_end {
                s2 + 1
            } else if looping {
                range_start + (s2 + 1 - range_end)
            } else {
                s2
            };

            for ch in 0..out_ch.min(src_ch) {
                let data = &buf.data[ch];
                let v0 = data.get(s0).copied().unwrap_or(0.0);
                let v1 = data.get(s1).copied().unwrap_or(0.0);
                let v2 = data.get(s2).copied().unwrap_or(0.0);
                let v3 = data.get(s3).copied().unwrap_or(0.0);

                let sample = cubic_interp(v0, v1, v2, v3, frac) * volume;
                if frame < out_buf[ch].len() {
                    out_buf[ch][frame] += sample;
                }
            }

            if src_ch == 1 && out_ch >= 2 {
                let data = &buf.data[0];
                let v0 = data.get(s0).copied().unwrap_or(0.0);
                let v1 = data.get(s1).copied().unwrap_or(0.0);
                let v2 = data.get(s2).copied().unwrap_or(0.0);
                let v3 = data.get(s3).copied().unwrap_or(0.0);

                let sample = cubic_interp(v0, v1, v2, v3, frac) * volume;
                if frame < out_buf[1].len() {
                    out_buf[1][frame] += sample;
                }
            }

            buf.playback_pos += ratio;
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

    pub(super) fn process_drum_machine_node(&self, node_id: NodeId, num_frames: usize) {
        let (bpm, volume, playing, pattern_idx) = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::DrumMachine(state) => {
                    (state.bpm, state.volume, state.playing, state.pattern)
                }
                _ => return,
            }
        };

        if !playing {
            return;
        }

        let node = self.nodes.get(&node_id).unwrap();
        let mut output_buffers = node.output_buffers.lock();
        let mut phase = node.drum_phase.lock();
        let mut step = node.drum_step.lock();
        let sample_rate = self.sample_rate;
        let samples_per_step = sample_rate * 60.0 / bpm / 4.0;
        let pattern = drum_pattern(pattern_idx);

        let Some(out_buf) = output_buffers.get_mut(0) else {
            return;
        };
        let out_ch = out_buf.len();

        for frame in 0..num_frames {
            let current_step = *step % DRUM_STEPS;
            let hit = pattern[current_step as usize];
            let step_phase = *phase;
            let step_sample = step_phase as usize;

            let mut sample_l = 0.0f32;
            let mut sample_r = 0.0f32;

            if hit & 1 != 0 && step_sample < DRUM_KICK_DECAY {
                let t = step_sample as f64;
                let env = (1.0 - t / DRUM_KICK_DECAY as f64).max(0.0);
                let freq = DRUM_KICK_FREQ * (1.0 - 0.5 * (t / DRUM_KICK_DECAY as f64).min(1.0));
                sample_l +=
                    (2.0 * std::f64::consts::PI * freq * t / sample_rate).sin() as f32 * env as f32;
                sample_r = sample_l;
            }
            if hit & 2 != 0 && step_sample < DRUM_SNARE_DECAY {
                let t = step_sample as f64;
                let env = (1.0 - t / DRUM_SNARE_DECAY as f64).max(0.0);
                let tone =
                    (2.0 * std::f64::consts::PI * DRUM_SNARE_FREQ * t / sample_rate).sin() as f32;
                let noise = ((t * 12345.6789).sin() * 2.0 - 1.0) as f32;
                let s = (tone * 0.4 + noise * 0.6) * env as f32;
                sample_l += s;
                sample_r += s;
            }
            if hit & 4 != 0 && step_sample < DRUM_HH_DECAY {
                let t = step_sample as f64;
                let env = (1.0 - t / DRUM_HH_DECAY as f64).max(0.0);
                let noise = ((t * 98765.4321).sin() * 2.0 - 1.0) as f32;
                let s = noise * env as f32 * 0.5;
                sample_l += s;
                sample_r += s;
            }

            let out_sample_l = sample_l * volume;
            let out_sample_r = sample_r * volume;

            if out_ch > 0 && frame < out_buf[0].len() {
                out_buf[0][frame] += out_sample_l;
            }
            if out_ch > 1 && frame < out_buf[1].len() {
                out_buf[1][frame] += out_sample_r;
            }

            *phase += 1.0;
            if *phase >= samples_per_step {
                *phase -= samples_per_step;
                *step = (*step + 1) % DRUM_STEPS;
            }
        }
    }

    pub(super) fn process_recorder_node(&self, node_id: NodeId, num_frames: usize) {
        let recording = {
            let node = self.nodes.get(&node_id).unwrap();
            match &node.internal_state {
                NodeInternalState::Recorder(state) => state.recording,
                _ => false,
            }
        };

        if !recording {
            return;
        }

        let input_data: Option<Vec<Vec<f32>>> = {
            let node = self.nodes.get(&node_id).unwrap();
            let input_buffers = node.input_buffers.lock();
            input_buffers.first().and_then(|opt| opt.clone())
        };

        let node = self.nodes.get(&node_id).unwrap();
        let mut buf = node.recorder_buffer.lock();
        if let Some(ref mut recorder) = *buf {
            if let Some(ref input) = input_data {
                let ch_count = recorder.len().min(input.len());
                for ch in 0..ch_count {
                    let len = num_frames.min(input[ch].len());
                    recorder[ch].extend_from_slice(&input[ch][..len]);
                }
            }
        }
    }
}
