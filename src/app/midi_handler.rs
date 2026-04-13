use super::ToneDockApp;
use crate::audio::node::{
    BackingTrackNodeState, LooperNodeState, MetronomeNodeState, NodeInternalState,
};
use crate::midi::{MidiAction, MidiBindingKey, MidiMessageType, TriggerMode};

impl ToneDockApp {
    pub(crate) fn poll_midi(&mut self) {
        let messages = self.midi_input.try_recv_messages();

        for msg in &messages {
            let key = MidiBindingKey {
                channel: msg.channel,
                message_type: msg.message_type,
                data_byte: msg.data_byte,
            };

            if self.midi_learning {
                if let Some(target) = self.midi_learn_target {
                    let mode = if msg.message_type == MidiMessageType::ControlChange {
                        TriggerMode::Momentary
                    } else {
                        TriggerMode::Toggle
                    };
                    self.midi_map.set_binding(key, target, mode);
                    self.midi_learning = false;
                    self.midi_learn_target = None;
                    self.status_message = self.i18n.trf(
                        "status.midi_learn_bound",
                        &[("action", target.label()), ("binding", &key.display())],
                    );
                    return;
                }
            }

            if let Some((action, _mode)) = self.midi_map.find_action(&key) {
                let value = msg.value;
                self.execute_midi_action(action, value);
            }
        }
    }

    fn execute_midi_action(&mut self, action: MidiAction, value: u8) {
        match action {
            MidiAction::PresetUp => {
                self.status_message = self.i18n.tr("status.midi_preset_up").into();
            }
            MidiAction::PresetDown => {
                self.status_message = self.i18n.tr("status.midi_preset_down").into();
            }
            MidiAction::LooperRecord => {
                if self.looper_node_id.is_none() {
                    self.looper_node_id = Some(self.audio_engine.add_looper_node());
                }
                self.looper_enabled = true;
                self.looper_recording = !self.looper_recording;
                self.looper_playing = !self.looper_recording;
                if let Some(id) = self.looper_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Looper(LooperNodeState {
                            enabled: true,
                            recording: self.looper_recording,
                            playing: self.looper_playing,
                            overdubbing: false,
                            cleared: false,
                            fixed_length_beats: None,
                            quantize_start: false,
                            pre_fader: self.looper_pre_fader,
                            active_track: self.looper_active_track,
                        }),
                    );
                    self.audio_engine.graph_set_enabled(id, true);
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::LooperStop => {
                self.looper_recording = false;
                self.looper_playing = false;
                self.looper_overdubbing = false;
                if let Some(id) = self.looper_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Looper(LooperNodeState {
                            enabled: self.looper_enabled,
                            recording: false,
                            playing: false,
                            overdubbing: false,
                            cleared: false,
                            fixed_length_beats: None,
                            quantize_start: false,
                            pre_fader: self.looper_pre_fader,
                            active_track: self.looper_active_track,
                        }),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::LooperPlay => {
                if self.looper_node_id.is_none() {
                    self.looper_node_id = Some(self.audio_engine.add_looper_node());
                }
                self.looper_playing = !self.looper_playing;
                self.looper_recording = false;
                if let Some(id) = self.looper_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Looper(LooperNodeState {
                            enabled: true,
                            recording: false,
                            playing: self.looper_playing,
                            overdubbing: self.looper_overdubbing,
                            cleared: false,
                            fixed_length_beats: None,
                            quantize_start: false,
                            pre_fader: self.looper_pre_fader,
                            active_track: self.looper_active_track,
                        }),
                    );
                    self.audio_engine.graph_set_enabled(id, true);
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::LooperOverdub => {
                self.looper_overdubbing = if self.looper_overdubbing {
                    false
                } else if self.looper_playing {
                    true
                } else {
                    false
                };
                if let Some(id) = self.looper_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Looper(LooperNodeState {
                            enabled: true,
                            recording: self.looper_recording,
                            playing: self.looper_playing,
                            overdubbing: self.looper_overdubbing,
                            cleared: false,
                            fixed_length_beats: None,
                            quantize_start: false,
                            pre_fader: self.looper_pre_fader,
                            active_track: self.looper_active_track,
                        }),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::LooperClear => {
                self.looper_recording = false;
                self.looper_playing = false;
                self.looper_overdubbing = false;
                if let Some(id) = self.looper_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Looper(LooperNodeState {
                            enabled: false,
                            recording: false,
                            playing: false,
                            overdubbing: false,
                            cleared: true,
                            fixed_length_beats: None,
                            quantize_start: false,
                            pre_fader: self.looper_pre_fader,
                            active_track: self.looper_active_track,
                        }),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::LooperUndo => {
                self.perform_undo();
            }
            MidiAction::TapTempo => {
                self.tap_tempo();
            }
            MidiAction::BackingPlay => {
                if let Some(id) = self.backing_track_node_id {
                    self.backing_track_playing = !self.backing_track_playing;
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::BackingTrack(BackingTrackNodeState {
                            playing: self.backing_track_playing,
                            volume: self.backing_track_volume,
                            speed: self.backing_track_speed,
                            looping: self.backing_track_looping,
                            file_loaded: true,
                            loop_start: None,
                            loop_end: None,
                            pitch_semitones: self.backing_track_pitch_semitones,
                            pre_roll_secs: self.backing_track_pre_roll_secs,
                            section_markers: vec![],
                        }),
                    );
                    self.audio_engine.graph_set_enabled(id, true);
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::BackingStop => {
                if let Some(id) = self.backing_track_node_id {
                    self.backing_track_playing = false;
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::BackingTrack(BackingTrackNodeState {
                            playing: false,
                            volume: self.backing_track_volume,
                            speed: self.backing_track_speed,
                            looping: self.backing_track_looping,
                            file_loaded: true,
                            loop_start: None,
                            loop_end: None,
                            pitch_semitones: self.backing_track_pitch_semitones,
                            pre_roll_secs: self.backing_track_pre_roll_secs,
                            section_markers: vec![],
                        }),
                    );
                    self.audio_engine.backing_track_seek(id, 0.0);
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::MetronomeToggle => {
                self.metronome_enabled = !self.metronome_enabled;
                if self.metronome_node_id.is_none() && self.metronome_enabled {
                    self.metronome_node_id = Some(self.audio_engine.add_metronome_node());
                }
                if let Some(id) = self.metronome_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Metronome(MetronomeNodeState {
                            bpm: self.metronome_bpm,
                            volume: self.metronome_volume,
                            count_in_beats: 0,
                            count_in_active: false,
                        }),
                    );
                    self.audio_engine
                        .graph_set_enabled(id, self.metronome_enabled);
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
            }
            MidiAction::PanicMute => {
                *self.audio_engine.master_volume.lock() = 0.0;
                self.master_volume = 0.0;
                self.status_message = self.i18n.tr("status.panic_mute").into();
            }
            MidiAction::MasterVolumeUp => {
                let new_vol = (self.master_volume + 0.05).min(1.0);
                self.master_volume = new_vol;
                *self.audio_engine.master_volume.lock() = new_vol;
            }
            MidiAction::MasterVolumeDown => {
                let new_vol = (self.master_volume - 0.05).max(0.0);
                self.master_volume = new_vol;
                *self.audio_engine.master_volume.lock() = new_vol;
            }
            MidiAction::ToggleBypassSelected => {
                if let Some(id) = self
                    .selected_rack_node
                    .or_else(|| self.node_editor.selected_node())
                {
                    let guard = self.audio_engine.graph.load();
                    let bypassed = guard.get_node(id).map(|n| n.bypassed).unwrap_or(false);
                    drop(guard);
                    self.audio_engine.graph_set_bypassed(id, !bypassed);
                    self.audio_engine.apply_commands_to_staging();
                }
            }
        }
        let _ = value;
    }

    pub(crate) fn tap_tempo(&mut self) {
        let now = std::time::Instant::now();
        self.tap_tempo_times.push(now);

        if self.tap_tempo_times.len() > 4 {
            self.tap_tempo_times.remove(0);
        }

        if self.tap_tempo_times.len() >= 2 {
            let intervals: Vec<f64> = self
                .tap_tempo_times
                .windows(2)
                .map(|w| w[1].duration_since(w[0]).as_secs_f64())
                .collect();
            let avg_interval = intervals.iter().sum::<f64>() / intervals.len() as f64;
            if avg_interval > 0.0 && avg_interval < 10.0 {
                let bpm = 60.0 / avg_interval;
                self.metronome_bpm = bpm.round().clamp(40.0, 300.0);
                if let Some(id) = self.metronome_node_id {
                    self.audio_engine.graph_set_state(
                        id,
                        NodeInternalState::Metronome(MetronomeNodeState {
                            bpm: self.metronome_bpm,
                            volume: self.metronome_volume,
                            count_in_beats: 0,
                            count_in_active: false,
                        }),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
                self.status_message = self.i18n.trf(
                    "status.tap_tempo",
                    &[("bpm", &format!("{:.0}", self.metronome_bpm))],
                );
            }
        }

        let last = self.tap_tempo_times.last().copied();
        if let Some(l) = last {
            self.tap_tempo_times
                .retain(|t| l.duration_since(*t).as_secs_f64() < 3.0);
        }
    }

    pub(crate) fn start_midi_learn(&mut self, action: MidiAction) {
        self.midi_learning = true;
        self.midi_learn_target = Some(action);
        self.status_message = self
            .i18n
            .trf("status.midi_learn_waiting", &[("action", action.label())]);
    }
}
