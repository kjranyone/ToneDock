use std::collections::HashMap;

use crate::audio::engine::AudioEngine;
use crate::audio::node::NodeId;
use crate::i18n::{I18n, Language};
use crate::ui::node_editor::NodeEditor;
use crate::ui::rack_view::RackView;
use crate::undo::UndoManager;
use crate::vst_host::editor::PluginEditor;
use crate::vst_host::scanner::PluginInfo;

mod commands;
mod dialogs;
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
        }
    }
}

const SETTINGS_KEY: &str = "tonedock_settings";

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
    looper_node_id: Option<NodeId>,

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
            looper_node_id: None,
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
        };

        *app.audio_engine.master_volume.lock() = app.master_volume;
        *app.audio_engine.input_gain.lock() = app.input_gain;

        app.scan_plugins();
        app.restore_audio_config();
        app.start_audio();

        app
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
        self.settings_dirty = true;
    }

    pub(crate) fn scan_plugins(&mut self) {
        let chain = self.audio_engine.chain.lock();
        match chain.scan_plugins() {
            Ok(plugins) => {
                self.available_plugins = plugins;
                self.status_message = self.i18n.trf(
                    "status.found_plugins",
                    &[("count", &self.available_plugins.len().to_string())],
                );
                log::info!("Scanned {} plugins", self.available_plugins.len());
            }
            Err(e) => {
                self.status_message = self
                    .i18n
                    .trf("status.scan_error", &[("error", &e.to_string())]);
                log::error!("Plugin scan failed: {}", e);
            }
        }
    }

    pub(crate) fn scan_plugins_with_custom_paths(&mut self) {
        let mut scanner = crate::vst_host::scanner::PluginScanner::new();
        for path in &self.custom_plugin_paths {
            scanner.add_path(path.clone());
        }
        let custom_plugins = scanner.scan();

        let chain = self.audio_engine.chain.lock();
        match chain.scan_plugins() {
            Ok(plugins) => {
                self.available_plugins = plugins;
                let mut seen: std::collections::HashSet<std::path::PathBuf> = self
                    .available_plugins
                    .iter()
                    .map(|p| p.path.clone())
                    .collect();
                for p in custom_plugins {
                    if seen.insert(p.path.clone()) {
                        self.available_plugins.push(p);
                    }
                }
                self.status_message = self.i18n.trf(
                    "status.found_plugins",
                    &[("count", &self.available_plugins.len().to_string())],
                );
                log::info!(
                    "Scanned {} plugins (with custom paths)",
                    self.available_plugins.len()
                );
            }
            Err(e) => {
                self.status_message = self
                    .i18n
                    .trf("status.scan_error", &[("error", &e.to_string())]);
                log::error!("Plugin scan failed: {}", e);
            }
        }
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

    pub(crate) fn set_language(&mut self, lang: Language) {
        self.i18n = I18n::new(lang);
    }
}
