use super::autosave_path;
use super::ToneDockApp;
use crate::audio::node::{NodeId, NodeInternalState, NodeType};
use crate::session::{Preset, Session, TransportState};

impl ToneDockApp {
    pub(crate) fn save_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                self.i18n.tr("file.tonedock_preset"),
                &["tonedock-preset.json"],
            )
            .set_file_name(self.i18n.tr("file.default_filename"))
            .save_file()
        {
            let preset = self.build_preset();
            let preset_name = preset.name.clone();
            let p = path.clone();
            std::thread::spawn(move || {
                if let Err(e) = preset.save_to_file(&p) {
                    log::error!("Preset save failed: {}", e);
                }
            });
            self.preset_name = preset_name;
            self.settings.last_session_path = Some(path.clone());
            self.settings_dirty = true;
            self.status_message = self.i18n.trf(
                "status.preset_saved",
                &[("path", &path.display().to_string())],
            );
        }
    }

    pub(crate) fn load_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                self.i18n.tr("file.tonedock_preset"),
                &["tonedock-preset.json", "tonedock.json"],
            )
            .pick_file()
        {
            let preset = match Preset::load_from_file(&path) {
                Ok(preset) => preset,
                Err(preset_err) => match Session::load_from_file(&path) {
                    Ok(session) => session.preset,
                    Err(session_err) => {
                        self.status_message = self.i18n.trf(
                            "status.preset_load_error_both",
                            &[
                                ("err1", &preset_err.to_string()),
                                ("err2", &session_err.to_string()),
                            ],
                        );
                        return;
                    }
                },
            };

            if let Err(err) = self.audio_engine.load_serialized_graph(&preset.graph) {
                self.status_message = self
                    .i18n
                    .trf("status.preset_load_error", &[("error", &err.to_string())]);
                return;
            }

            self.close_all_rack_editors();
            self.audio_engine.chain_node_ids.clear();
            self.rack_order = preset.rack_order.clone();
            self.select_rack_plugin_node(None);
            self.preset_name = preset.name.clone();
            self.sync_transport_state_from_graph();
            self.apply_transport_state(&preset.transport);
            self.settings.last_session_path = Some(path);
            self.settings_dirty = true;
        }
    }

    pub(crate) fn save_workspace(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.i18n.tr("file.tonedock_session"), &["tonedock.json"])
            .save_file()
        {
            let preset = self.build_preset();
            let session = crate::session::Session {
                name: self.preset_name.clone(),
                sample_rate: self.audio_engine.sample_rate,
                buffer_size: self.audio_engine.buffer_size,
                preset,
                chain: Vec::new(),
                graph: None,
            };
            match session.save_to_file(&path) {
                Ok(()) => {
                    self.status_message = self.i18n.trf(
                        "status.workspace_saved",
                        &[("path", &path.display().to_string())],
                    );
                }
                Err(e) => {
                    self.status_message = self
                        .i18n
                        .trf("status.workspace_save_error", &[("error", &e.to_string())]);
                }
            }
        }
    }

    pub(crate) fn load_workspace(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.i18n.tr("file.tonedock_session"), &["tonedock.json"])
            .pick_file()
        {
            match Session::load_from_file(&path) {
                Ok(session) => {
                    if let Err(err) = self
                        .audio_engine
                        .load_serialized_graph(&session.preset.graph)
                    {
                        self.status_message = self.i18n.trf(
                            "status.session_import_error",
                            &[("error", &err.to_string())],
                        );
                        return;
                    }
                    self.close_all_rack_editors();
                    self.audio_engine.chain_node_ids.clear();
                    self.rack_order = session.preset.rack_order.clone();
                    self.select_rack_plugin_node(None);
                    self.preset_name = session.preset.name;
                    self.sync_transport_state_from_graph();
                    self.apply_transport_state(&session.preset.transport);
                    self.settings.last_session_path = Some(path);
                    self.settings_dirty = true;
                    self.status_message = self.i18n.tr("status.workspace_loaded").into();
                }
                Err(err) => {
                    self.status_message = self.i18n.trf(
                        "status.session_import_error",
                        &[("error", &err.to_string())],
                    );
                }
            }
        }
    }

    pub(crate) fn snapshot_to_ab(&mut self, slot: char) {
        let preset = self.build_preset();
        let json = serde_json::to_string(&preset).unwrap_or_default();
        match slot {
            'a' => self.preset_a = Some(json),
            'b' => self.preset_b = Some(json),
            _ => {}
        }
    }

    pub(crate) fn restore_ab(&mut self, slot: char) {
        let json = match slot {
            'a' => self.preset_a.as_ref(),
            'b' => self.preset_b.as_ref(),
            _ => None,
        };
        if let Some(json) = json {
            if let Ok(preset) = serde_json::from_str::<Preset>(json) {
                if let Err(err) = self.audio_engine.load_serialized_graph(&preset.graph) {
                    self.status_message = self
                        .i18n
                        .trf("status.ab_restore_error", &[("error", &err.to_string())]);
                    return;
                }
                self.close_all_rack_editors();
                self.audio_engine.chain_node_ids.clear();
                self.rack_order = preset.rack_order.clone();
                self.select_rack_plugin_node(None);
                self.preset_name = preset.name.clone();
                self.sync_transport_state_from_graph();
                self.apply_transport_state(&preset.transport);
                let label = if slot == 'a' { 'A' } else { 'B' };
                self.status_message = self
                    .i18n
                    .trf("status.ab_restored", &[("slot", &label.to_string())]);
            }
        }
    }

    pub(crate) fn suggest_plugin_chain(&self) -> Vec<String> {
        let mut suggestions = Vec::new();
        let has_plugins = !self.available_plugins.is_empty();
        if has_plugins {
            let names: Vec<String> = self
                .available_plugins
                .iter()
                .map(|p| p.name.clone())
                .collect();
            if !names.is_empty() {
                suggestions.push(format!("Clean: {} → Reverb", names[0]));
            }
            if names.len() > 1 {
                suggestions.push(format!("Blues: {} → {} → Delay", names[0], names[1]));
            }
            if names.len() > 2 {
                suggestions.push(format!("Metal: {} → {} → {}", names[0], names[1], names[2]));
            }
            suggestions.push("Basic: Tuner → Amp → Cabinet".to_string());
            suggestions.push("Practice: Amp → Looper → Metronome".to_string());
        } else {
            suggestions.push("Install VST3 plugins for suggestions".to_string());
        }
        suggestions
    }

    pub(crate) fn build_preset(&self) -> Preset {
        let preset_name = if self.preset_name.is_empty() {
            self.i18n.tr("file.untitled").into()
        } else {
            self.preset_name.clone()
        };

        Preset {
            name: preset_name,
            graph: self.audio_engine.snapshot_serialized_graph(),
            rack_order: self.rack_order.clone(),
            transport: TransportState {
                metronome_bpm: Some(self.metronome_bpm),
                metronome_volume: Some(self.metronome_volume),
                metronome_enabled: Some(self.metronome_enabled),
                backing_track_volume: Some(self.backing_track_volume),
                backing_track_speed: Some(self.backing_track_speed),
                backing_track_looping: Some(self.backing_track_looping),
                backing_track_pitch_semitones: Some(self.backing_track_pitch_semitones),
                backing_track_pre_roll_secs: Some(self.backing_track_pre_roll_secs),
                looper_pre_fader: Some(self.looper_pre_fader),
                master_volume: Some(self.master_volume),
                input_gain: Some(self.input_gain),
                audio_host_id: self.audio_engine.host_id.map(|id| format!("{:?}", id)),
                input_device: self.audio_engine.input_device_name.clone(),
                output_device: self.audio_engine.output_device_name.clone(),
                sample_rate: Some(self.audio_engine.sample_rate as u32),
                buffer_size: Some(self.audio_engine.buffer_size),
            },
        }
    }

    pub(crate) fn import_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(self.i18n.tr("file.tonedock_session"), &["tonedock.json"])
            .pick_file()
        {
            match Session::load_from_file(&path) {
                Ok(session) => {
                    if let Err(err) = self
                        .audio_engine
                        .load_serialized_graph(&session.preset.graph)
                    {
                        self.status_message = self.i18n.trf(
                            "status.session_import_error",
                            &[("error", &err.to_string())],
                        );
                        return;
                    }

                    self.close_all_rack_editors();
                    self.audio_engine.chain_node_ids.clear();
                    self.rack_order = session.preset.rack_order.clone();
                    self.select_rack_plugin_node(None);
                    self.preset_name = session.preset.name;
                    self.sync_transport_state_from_graph();
                    self.settings.last_session_path = Some(path.clone());
                    self.settings_dirty = true;
                    self.status_message = self.i18n.trf(
                        "status.imported_preset",
                        &[("path", &path.display().to_string())],
                    );
                }
                Err(err) => {
                    self.status_message = self.i18n.trf(
                        "status.session_import_error",
                        &[("error", &err.to_string())],
                    );
                }
            }
        }
    }

    pub(crate) fn sync_transport_state_from_graph(&mut self) {
        self.metronome_enabled = false;
        self.metronome_bpm = 120.0;
        self.metronome_volume = 0.5;
        self.metronome_node_id = None;

        self.looper_enabled = false;
        self.looper_recording = false;
        self.looper_playing = false;
        self.looper_overdubbing = false;
        self.looper_node_id = None;

        let guard = self.audio_engine.graph.load();
        let mut node_ids: Vec<NodeId> = guard.nodes().keys().copied().collect();
        node_ids.sort();

        for node_id in node_ids {
            let Some(node) = guard.get_node(node_id) else {
                continue;
            };

            match (&node.node_type, &node.internal_state) {
                (NodeType::Metronome, NodeInternalState::Metronome(state)) => {
                    self.metronome_node_id = Some(node_id);
                    self.metronome_enabled = node.enabled;
                    self.metronome_bpm = state.bpm;
                    self.metronome_volume = state.volume;
                }
                (NodeType::Looper, NodeInternalState::Looper(state)) => {
                    self.looper_node_id = Some(node_id);
                    self.looper_enabled = node.enabled && state.enabled;
                    self.looper_recording = state.recording;
                    self.looper_playing = state.playing;
                    self.looper_overdubbing = state.overdubbing;
                    self.looper_pre_fader = state.pre_fader;
                }
                (NodeType::Looper, _) => {
                    self.looper_node_id = Some(node_id);
                    self.looper_enabled = node.enabled;
                }
                _ => {}
            }
        }
    }

    pub(crate) fn apply_transport_state(&mut self, transport: &TransportState) {
        if let Some(bpm) = transport.metronome_bpm {
            self.metronome_bpm = bpm;
        }
        if let Some(vol) = transport.metronome_volume {
            self.metronome_volume = vol;
        }
        if let Some(enabled) = transport.metronome_enabled {
            self.metronome_enabled = enabled;
        }
        if let Some(vol) = transport.backing_track_volume {
            self.backing_track_volume = vol;
        }
        if let Some(speed) = transport.backing_track_speed {
            self.backing_track_speed = speed;
        }
        if let Some(looping) = transport.backing_track_looping {
            self.backing_track_looping = looping;
        }
        if let Some(pitch) = transport.backing_track_pitch_semitones {
            self.backing_track_pitch_semitones = pitch;
        }
        if let Some(pre_roll) = transport.backing_track_pre_roll_secs {
            self.backing_track_pre_roll_secs = pre_roll;
        }
        if let Some(pre_fader) = transport.looper_pre_fader {
            self.looper_pre_fader = pre_fader;
        }
        if let Some(vol) = transport.master_volume {
            self.master_volume = vol;
            *self.audio_engine.master_volume.lock() = vol;
        }
        if let Some(gain) = transport.input_gain {
            self.input_gain = gain;
            *self.audio_engine.input_gain.lock() = gain;
        }
        if transport.output_device.is_some()
            || transport.sample_rate.is_some()
            || transport.buffer_size.is_some()
        {
            let host_id = transport.audio_host_id.as_deref().and_then(|s| match s {
                "Asio" => Some(cpal::HostId::Asio),
                "Wasapi" => Some(cpal::HostId::Wasapi),
                _ => None,
            });
            let sr = transport
                .sample_rate
                .unwrap_or(self.audio_engine.sample_rate as u32);
            let bs = transport
                .buffer_size
                .unwrap_or(self.audio_engine.buffer_size);
            let in_ch = self.audio_engine.input_channel;
            let out_ch = self.audio_engine.output_channels;
            if let Err(e) = self.audio_engine.restart_with_config(
                host_id,
                transport.input_device.as_deref(),
                transport.output_device.as_deref(),
                sr,
                bs,
                in_ch,
                out_ch,
            ) {
                log::warn!("Failed to restore audio config from preset: {}", e);
            }
        }
    }

    pub(crate) fn open_preferences(&mut self) {
        self.show_preferences = true;
        self.preferences_state = Some(crate::ui::preferences::PreferencesState::new(
            self.audio_engine.current_host_id(),
            self.audio_engine.sample_rate as u32,
            self.audio_engine.buffer_size,
            self.custom_plugin_paths.clone(),
            self.audio_engine.input_channel,
            self.audio_engine.output_channels,
            self.inline_rack_plugin_gui,
        ));
    }

    pub(crate) fn autosave(&self) {
        let Some(path) = autosave_path() else {
            log::warn!("Could not determine autosave directory");
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::error!("Failed to create autosave directory: {}", e);
                return;
            }
        }
        let preset = self.build_preset();
        if let Err(e) = preset.save_to_file(&path) {
            log::error!("Autosave failed: {}", e);
        }
    }

    pub(crate) fn auto_restore(&mut self) {
        let preset = autosave_path().and_then(|path| {
            if path.exists() {
                Preset::load_from_file(&path).ok()
            } else {
                None
            }
        });

        let Some(preset) = preset else {
            return;
        };

        if preset.graph.nodes.is_empty() && preset.graph.connections.is_empty() {
            return;
        }

        if let Err(err) = self.audio_engine.load_serialized_graph(&preset.graph) {
            log::warn!("Auto-restore failed: {}", err);
            self.status_message = self
                .i18n
                .trf("status.auto_restore_failed", &[("error", &err.to_string())]);
            return;
        }

        self.audio_engine.chain_node_ids.clear();
        self.rack_order = preset.rack_order.clone();
        self.select_rack_plugin_node(None);
        self.preset_name = preset.name.clone();
        self.sync_transport_state_from_graph();
        self.apply_transport_state(&preset.transport);
        self.status_message = self.i18n.tr("status.auto_restored").into();
        log::info!("Auto-restored previous session");
    }
}
