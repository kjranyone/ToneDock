use super::ToneDockApp;
use crate::audio::node::{NodeId, NodeInternalState, NodeType};
use crate::session::{Preset, Session};

impl ToneDockApp {
    pub(crate) fn save_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Preset", &["tonedock-preset.json"])
            .set_file_name("preset.tonedock-preset.json")
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
            self.status_message = format!("Preset saved to {}", path.display());
        }
    }

    pub(crate) fn load_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "ToneDock Preset",
                &["tonedock-preset.json", "tonedock.json"],
            )
            .pick_file()
        {
            let preset = match Preset::load_from_file(&path) {
                Ok(preset) => preset,
                Err(preset_err) => match Session::load_from_file(&path) {
                    Ok(session) => session.preset,
                    Err(session_err) => {
                        self.status_message =
                            format!("Preset load error: {} / {}", preset_err, session_err);
                        return;
                    }
                },
            };

            if let Err(err) = self.audio_engine.load_serialized_graph(&preset.graph) {
                self.status_message = format!("Preset load error: {}", err);
                return;
            }

            self.close_all_rack_editors();
            self.audio_engine.chain_node_ids.clear();
            self.rack_order = preset.rack_order.clone();
            self.select_rack_plugin_node(None);
            self.preset_name = preset.name.clone();
            self.sync_transport_state_from_graph();
            self.status_message = format!("Preset loaded: {}", preset.name);
        }
    }

    pub(crate) fn build_preset(&self) -> Preset {
        let preset_name = if self.preset_name.is_empty() {
            "Untitled".into()
        } else {
            self.preset_name.clone()
        };

        Preset {
            name: preset_name,
            graph: self.audio_engine.snapshot_serialized_graph(),
            rack_order: self.rack_order.clone(),
        }
    }

    pub(crate) fn import_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Session", &["tonedock.json"])
            .pick_file()
        {
            match Session::load_from_file(&path) {
                Ok(session) => {
                    if let Err(err) = self
                        .audio_engine
                        .load_serialized_graph(&session.preset.graph)
                    {
                        self.status_message = format!("Session import error: {}", err);
                        return;
                    }

                    self.close_all_rack_editors();
                    self.audio_engine.chain_node_ids.clear();
                    self.rack_order = session.preset.rack_order.clone();
                    self.select_rack_plugin_node(None);
                    self.preset_name = session.preset.name;
                    self.sync_transport_state_from_graph();
                    self.status_message = format!("Imported preset from {}", path.display());
                }
                Err(err) => {
                    self.status_message = format!("Session import error: {}", err);
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
                }
                (NodeType::Looper, _) => {
                    self.looper_node_id = Some(node_id);
                    self.looper_enabled = node.enabled;
                }
                _ => {}
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
}
