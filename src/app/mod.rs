use std::collections::HashMap;

use crate::audio::engine::AudioEngine;
use crate::audio::node::NodeId;
use crate::ui::node_editor::NodeEditor;
use crate::ui::rack_view::RackView;
use crate::undo::UndoManager;
use crate::vst_host::editor::PluginEditor;
use crate::vst_host::scanner::PluginInfo;

mod commands;
mod rack;
mod rack_view;
mod session;
mod templates;
mod toolbar;

pub(super) enum ViewMode {
    Rack,
    NodeEditor,
}

pub struct ToneDockApp {
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
}

impl ToneDockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::ui::theme::apply_fonts(&cc.egui_ctx);
        crate::ui::theme::apply_style(&cc.egui_ctx);

        let audio_engine = AudioEngine::new().unwrap_or_else(|e| {
            log::error!("Failed to create audio engine: {}", e);
            panic!("Audio engine init failed: {}", e);
        });

        let mut app = Self {
            audio_engine,
            rack_view: RackView::new(),
            node_editor: NodeEditor::new(),
            view_mode: ViewMode::Rack,
            available_plugins: Vec::new(),
            custom_plugin_paths: Vec::new(),
            preset_name: "Untitled".into(),
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
            inline_rack_plugin_gui: false,
            inline_rack_editor_node: None,
            show_about: false,
            status_message: "Ready".into(),
            show_preferences: false,
            preferences_state: None,
            master_volume: 0.8,
            input_gain: 1.0,
            main_hwnd: None,
        };

        app.scan_plugins();
        app.start_audio();

        app
    }

    pub(crate) fn scan_plugins(&mut self) {
        let chain = self.audio_engine.chain.lock();
        match chain.scan_plugins() {
            Ok(plugins) => {
                self.available_plugins = plugins;
                self.status_message = format!("Found {} plugins", self.available_plugins.len());
                log::info!("Scanned {} plugins", self.available_plugins.len());
            }
            Err(e) => {
                self.status_message = format!("Scan error: {}", e);
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
                self.status_message = format!("Found {} plugins", self.available_plugins.len());
                log::info!(
                    "Scanned {} plugins (with custom paths)",
                    self.available_plugins.len()
                );
            }
            Err(e) => {
                self.status_message = format!("Scan error: {}", e);
                log::error!("Plugin scan failed: {}", e);
            }
        }
    }

    pub(crate) fn start_audio(&mut self) {
        if let Err(e) = self.audio_engine.start() {
            self.status_message = format!("Audio error: {}", e);
            log::error!("Audio start failed: {}", e);
        } else {
            self.status_message = "Audio running".into();
        }
    }
}
