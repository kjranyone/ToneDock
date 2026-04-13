use std::collections::HashMap;

use crate::audio::engine::AudioEngine;
use crate::audio::node::NodeId;
use crate::i18n::{I18n, Language};
use crate::midi::{MidiAction, MidiInput, MidiMap};
use crate::ui::node_editor::NodeEditor;
use crate::ui::rack_view::RackView;
use crate::undo::UndoManager;
use crate::vst_host::editor::PluginEditor;
use crate::vst_host::scanner::PluginInfo;

mod commands;
mod dialogs;
mod midi_handler;
mod rack;
mod rack_view;
mod session;
mod templates;
mod toolbar;
mod transport;

fn ser_host_id<S: serde::Serializer>(val: &Option<cpal::HostId>, s: S) -> Result<S::Ok, S::Error> {
    match val {
        Some(id) => s.serialize_some(id.name()),
        None => s.serialize_none(),
    }
}

fn de_host_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<cpal::HostId>, D::Error> {
    let name: Option<String> = serde::Deserialize::deserialize(d)?;
    Ok(name.and_then(|n| {
        cpal::platform::available_hosts()
            .into_iter()
            .find(|h| h.name() == n)
    }))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AppSettings {
    #[serde(
        serialize_with = "ser_host_id",
        deserialize_with = "de_host_id",
        default
    )]
    pub audio_host_id: Option<cpal::HostId>,
    pub input_device_name: Option<String>,
    pub output_device_name: Option<String>,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub input_channel: usize,
    pub output_channels: (usize, usize),
    pub master_volume: f32,
    pub input_gain: f32,
    pub custom_plugin_paths: Vec<std::path::PathBuf>,
    pub inline_rack_plugin_gui: bool,
    pub language: Language,
    pub last_session_path: Option<std::path::PathBuf>,
    #[serde(default)]
    pub midi_device_name: Option<String>,
    #[serde(default)]
    pub midi_map: MidiMap,
    #[serde(default)]
    pub disabled_plugin_paths: Vec<std::path::PathBuf>,
    #[serde(default)]
    pub bpm_goal: Option<f64>,
    #[serde(default)]
    pub total_practice_secs: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            audio_host_id: None,
            input_device_name: None,
            output_device_name: None,
            sample_rate: 48000,
            buffer_size: 256,
            input_channel: 0,
            output_channels: (0, 1),
            master_volume: 0.8,
            input_gain: 1.0,
            custom_plugin_paths: Vec::new(),
            inline_rack_plugin_gui: false,
            language: Language::default(),
            last_session_path: None,
            midi_device_name: None,
            midi_map: MidiMap::new(),
            disabled_plugin_paths: Vec::new(),
            bpm_goal: None,
            total_practice_secs: 0,
        }
    }
}

const SETTINGS_KEY: &str = "tonedock_settings";

pub(crate) fn autosave_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("ToneDock").join("autosave.tonedock-preset.json"))
}

pub(super) enum ViewMode {
    Rack,
    NodeEditor,
}

pub struct ToneDockApp {
    i18n: I18n,
    audio_engine: AudioEngine,
    rack_view: RackView,
    node_editor: NodeEditor,
    view_mode: ViewMode,
    available_plugins: Vec<PluginInfo>,
    custom_plugin_paths: Vec<std::path::PathBuf>,
    preset_name: String,
    undo_manager: UndoManager,

    metronome_enabled: bool,
    metronome_bpm: f64,
    metronome_volume: f32,
    metronome_node_id: Option<NodeId>,

    looper_enabled: bool,
    looper_recording: bool,
    looper_playing: bool,
    looper_overdubbing: bool,
    looper_pre_fader: bool,
    looper_active_track: u8,
    looper_node_id: Option<NodeId>,

    backing_track_node_id: Option<NodeId>,
    backing_track_playing: bool,
    backing_track_volume: f32,
    backing_track_speed: f32,
    backing_track_pitch_semitones: f32,
    backing_track_pre_roll_secs: f64,
    backing_track_looping: bool,
    backing_track_file_name: Option<String>,
    backing_track_duration: f64,

    recorder_node_id: Option<NodeId>,
    drum_machine_node_id: Option<NodeId>,

    preset_a: Option<String>,
    preset_b: Option<String>,

    selected_rack_node: Option<NodeId>,
    rack_order: Vec<NodeId>,
    rack_plugin_editors: HashMap<NodeId, PluginEditor>,
    inline_rack_plugin_gui: bool,
    inline_rack_editor_node: Option<NodeId>,
    show_about: bool,
    status_message: String,

    show_preferences: bool,
    preferences_state: Option<crate::ui::preferences::PreferencesState>,

    master_volume: f32,
    input_gain: f32,
    main_hwnd: Option<std::ptr::NonNull<std::ffi::c_void>>,
    settings: AppSettings,
    settings_dirty: bool,

    midi_input: MidiInput,
    midi_map: MidiMap,
    midi_learning: bool,
    midi_learn_target: Option<MidiAction>,
    tap_tempo_times: Vec<std::time::Instant>,

    fullscreen: bool,
    practice_timer_start: Option<std::time::Instant>,
    last_dropout_count: u64,
    disabled_plugin_paths: Vec<std::path::PathBuf>,

    scan_rx: Option<crossbeam_channel::Receiver<ScanResult>>,
    scanning_in_progress: bool,
}

enum ScanResult {
    Full(Vec<PluginInfo>),
    Delta(Vec<PluginInfo>),
}

impl ToneDockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::ui::theme::apply_fonts(&cc.egui_ctx);
        crate::ui::theme::apply_style(&cc.egui_ctx);

        let settings: AppSettings = cc
            .storage
            .and_then(|s| eframe::get_value(s, SETTINGS_KEY))
            .unwrap_or_default();

        let audio_engine = AudioEngine::new().unwrap_or_else(|e| {
            log::error!("Failed to create audio engine: {}", e);
            panic!("Audio engine init failed: {}", e);
        });

        let i18n = I18n::new(settings.language);
        let preset_name: String = i18n.tr("file.untitled").into();
        let initial_status: String = i18n.tr("status.ready").into();

        let midi_map = settings.midi_map.clone();
        let midi_device_name = settings.midi_device_name.clone();
        let disabled_plugin_paths = settings.disabled_plugin_paths.clone();

        let mut app = Self {
            i18n,
            audio_engine,
            rack_view: RackView::new(),
            node_editor: NodeEditor::new(),
            view_mode: ViewMode::Rack,
            available_plugins: Vec::new(),
            custom_plugin_paths: settings.custom_plugin_paths.clone(),
            preset_name,
            undo_manager: UndoManager::new(),
            metronome_enabled: false,
            metronome_bpm: 120.0,
            metronome_volume: 0.5,
            metronome_node_id: None,
            looper_enabled: false,
            looper_recording: false,
            looper_playing: false,
            looper_overdubbing: false,
            looper_pre_fader: false,
            looper_active_track: 0,
            looper_node_id: None,
            backing_track_node_id: None,
            backing_track_playing: false,
            backing_track_volume: 0.8,
            backing_track_speed: 1.0,
            backing_track_pitch_semitones: 0.0,
            backing_track_pre_roll_secs: 0.0,
            backing_track_looping: true,
            backing_track_file_name: None,
            backing_track_duration: 0.0,
            recorder_node_id: None,
            drum_machine_node_id: None,
            preset_a: None,
            preset_b: None,
            selected_rack_node: None,
            rack_order: Vec::new(),
            rack_plugin_editors: HashMap::new(),
            inline_rack_plugin_gui: settings.inline_rack_plugin_gui,
            inline_rack_editor_node: None,
            show_about: false,
            status_message: initial_status,
            show_preferences: false,
            preferences_state: None,
            master_volume: settings.master_volume,
            input_gain: settings.input_gain,
            main_hwnd: None,
            settings,
            settings_dirty: false,
            midi_input: MidiInput::new(),
            midi_map,
            midi_learning: false,
            midi_learn_target: None,
            tap_tempo_times: Vec::new(),
            fullscreen: false,
            practice_timer_start: None,
            last_dropout_count: 0,
            disabled_plugin_paths,
            scan_rx: None,
            scanning_in_progress: false,
        };

        *app.audio_engine.master_volume.lock() = app.master_volume;
        *app.audio_engine.input_gain.lock() = app.input_gain;

        app.scan_plugins();
        app.restore_audio_config();
        app.start_audio();
        app.restore_midi_device(&midi_device_name);
        app.auto_restore();

        app
    }

    fn restore_midi_device(&mut self, device_name: &Option<String>) {
        if let Some(name) = device_name {
            let devices = crate::midi::MidiInput::enumerate_devices();
            if let Some(device) = devices.iter().find(|d| d.name == *name) {
                if let Err(e) = self.midi_input.open_device(device.port_index) {
                    log::warn!("Failed to restore MIDI device '{}': {}", name, e);
                }
            }
        }
    }

    fn restore_audio_config(&mut self) {
        let s = &self.settings;
        if s.input_device_name.is_some() || s.output_device_name.is_some() {
            if let Err(e) = self.audio_engine.restart_with_config(
                s.audio_host_id,
                s.input_device_name.as_deref(),
                s.output_device_name.as_deref(),
                s.sample_rate,
                s.buffer_size,
                s.input_channel,
                s.output_channels,
            ) {
                log::warn!("Could not restore audio config: {}", e);
            }
        }
    }

    pub(crate) fn save_settings(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.settings_dirty {
            return;
        }
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
        self.settings_dirty = false;
    }

    pub(crate) fn sync_settings_from_engine(&mut self) {
        self.settings.audio_host_id = self.audio_engine.current_host_id();
        self.settings.input_device_name = self.audio_engine.current_input_device_name().clone();
        self.settings.output_device_name = self.audio_engine.current_output_device_name().clone();
        self.settings.sample_rate = self.audio_engine.sample_rate as u32;
        self.settings.buffer_size = self.audio_engine.buffer_size;
        self.settings.input_channel = self.audio_engine.input_channel;
        self.settings.output_channels = self.audio_engine.output_channels;
        self.settings.master_volume = self.master_volume;
        self.settings.input_gain = self.input_gain;
        self.settings.custom_plugin_paths = self.custom_plugin_paths.clone();
        self.settings.inline_rack_plugin_gui = self.inline_rack_plugin_gui;
        self.settings.language = self.i18n.language();
        self.settings.midi_map = self.midi_map.clone();
        self.settings.disabled_plugin_paths = self.disabled_plugin_paths.clone();
        self.settings_dirty = true;
    }

    pub(crate) fn scan_plugins(&mut self) {
        if self.scanning_in_progress {
            return;
        }
        self.scanning_in_progress = true;
        self.status_message = self.i18n.tr("status.scanning").into();

        let custom_paths = self.custom_plugin_paths.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.scan_rx = Some(rx);

        std::thread::spawn(move || {
            let mut scanner = crate::vst_host::scanner::PluginScanner::new();
            for path in &custom_paths {
                scanner.add_path(path.clone());
            }
            let plugins = scanner.scan();
            let _ = tx.send(ScanResult::Full(plugins));
        });
    }

    pub(crate) fn poll_scan_results(&mut self) {
        if let Some(ref rx) = self.scan_rx {
            if let Ok(result) = rx.try_recv() {
                self.scanning_in_progress = false;
                self.scan_rx = None;
                let disabled = self.disabled_plugin_paths.clone();
                match result {
                    ScanResult::Full(plugins) => {
                        self.available_plugins = plugins
                            .into_iter()
                            .filter(|p| !disabled.iter().any(|d| p.path.starts_with(d)))
                            .collect();
                        self.status_message = self.i18n.trf(
                            "status.found_plugins",
                            &[("count", &self.available_plugins.len().to_string())],
                        );
                        log::info!(
                            "Background scan found {} plugins",
                            self.available_plugins.len()
                        );
                    }
                    ScanResult::Delta(new_plugins) => {
                        if new_plugins.is_empty() {
                            self.status_message = self.i18n.tr("status.no_new_plugins").into();
                            return;
                        }
                        let count = new_plugins.len();
                        let mut seen: std::collections::HashSet<std::path::PathBuf> = self
                            .available_plugins
                            .iter()
                            .map(|p| p.path.clone())
                            .collect();
                        for p in new_plugins {
                            if !disabled.iter().any(|d| p.path.starts_with(d))
                                && seen.insert(p.path.clone())
                            {
                                self.available_plugins.push(p);
                            }
                        }
                        self.status_message = self
                            .i18n
                            .trf("status.delta_scan", &[("count", &count.to_string())]);
                    }
                }
            }
        }
    }

    pub(crate) fn scan_plugins_with_custom_paths(&mut self) {
        self.scan_plugins();
    }

    pub(crate) fn start_audio(&mut self) {
        if let Err(e) = self.audio_engine.start() {
            self.status_message = self
                .i18n
                .trf("status.audio_error", &[("error", &e.to_string())]);
            log::error!("Audio start failed: {}", e);
        } else {
            self.status_message = self.i18n.tr("status.audio_running").into();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn rescan_delta(&mut self) {
        if self.scanning_in_progress {
            return;
        }
        self.scanning_in_progress = true;
        self.status_message = self.i18n.tr("status.scanning").into();

        let existing = self.available_plugins.clone();
        let custom_paths = self.custom_plugin_paths.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.scan_rx = Some(rx);

        std::thread::spawn(move || {
            let mut scanner = crate::vst_host::scanner::PluginScanner::new();
            for path in &custom_paths {
                scanner.add_path(path.clone());
            }
            let new_plugins = scanner.scan_delta(&existing);
            let _ = tx.send(ScanResult::Delta(new_plugins));
        });
    }

    pub(crate) fn set_language(&mut self, lang: Language) {
        self.i18n = I18n::new(lang);
    }
}
