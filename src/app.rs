use std::sync::Arc;

use eframe::App;
use egui::*;

use crate::audio::engine::AudioEngine;
use crate::audio::node::{
    ChannelConfig, Connection, LooperNodeState, MetronomeNodeState, NodeId, NodeInternalState,
    NodeType, PortId,
};
use crate::session::{Preset, Session};
use crate::ui::node_editor::{EdCmd, NodeEditor, NodeSnap};
use crate::ui::preferences::{PreferencesResult, PreferencesState};
use crate::ui::rack_view::RackView;
use crate::undo::{UndoAction, UndoManager, UndoStep};
use crate::vst_host::scanner::PluginInfo;

enum ViewMode {
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
    session: Session,
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

    selected_chain_slot: Option<usize>,
    show_about: bool,
    status_message: String,

    show_preferences: bool,
    preferences_state: Option<PreferencesState>,

    master_volume: f32,
    input_gain: f32,
    main_hwnd: Option<std::ptr::NonNull<std::ffi::c_void>>,
}

fn ui_section_frame() -> Frame {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(18, 18, 22, 210))
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 12),
        ))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 6))
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
            session: Session::default(),
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
            selected_chain_slot: None,
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

    fn scan_plugins(&mut self) {
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

    fn scan_plugins_with_custom_paths(&mut self) {
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

    fn start_audio(&mut self) {
        if let Err(e) = self.audio_engine.start() {
            self.status_message = format!("Audio error: {}", e);
            log::error!("Audio start failed: {}", e);
        } else {
            self.status_message = "Audio running".into();
        }
    }

    fn save_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Session", &["tonedock.json"])
            .set_file_name("session.tonedock.json")
            .save_file()
        {
            let session = self.build_session();
            let p = path.clone();
            std::thread::spawn(move || {
                if let Err(e) = session.save_to_file(&p) {
                    log::error!("Save failed: {}", e);
                }
            });
            self.status_message = format!("Saved to {}", path.display());
        }
    }

    fn save_preset(&mut self) {
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
            self.session.preset.name = preset_name;
            self.status_message = format!("Preset saved to {}", path.display());
        }
    }

    fn load_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Session", &["tonedock.json"])
            .pick_file()
        {
            match Session::load_from_file(&path) {
                Ok(session) => {
                    let host_id = self.audio_engine.current_host_id();
                    let input_name = self
                        .audio_engine
                        .current_input_device_name()
                        .map(str::to_owned);
                    let output_name = self
                        .audio_engine
                        .current_output_device_name()
                        .map(str::to_owned);

                    if let Err(err) = self.audio_engine.restart_with_config(
                        host_id,
                        input_name.as_deref(),
                        output_name.as_deref(),
                        session.sample_rate as u32,
                        session.buffer_size,
                        self.audio_engine.input_channel,
                        self.audio_engine.output_channels,
                    ) {
                        self.status_message = format!("Load error: {}", err);
                        return;
                    }

                    if !session.preset.graph.nodes.is_empty()
                        || !session.preset.graph.connections.is_empty()
                    {
                        if let Err(err) = self
                            .audio_engine
                            .load_serialized_graph(&session.preset.graph)
                        {
                            self.status_message = format!("Load error: {}", err);
                            return;
                        }
                    }

                    self.sync_transport_state_from_graph();
                    self.status_message = format!("Loaded: {}", session.name);
                    self.session = session;
                }
                Err(e) => {
                    self.status_message = format!("Load error: {}", e);
                }
            }
        }
    }

    fn load_preset(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Preset", &["tonedock-preset.json"])
            .pick_file()
        {
            match Preset::load_from_file(&path) {
                Ok(preset) => {
                    if let Err(err) = self.audio_engine.load_serialized_graph(&preset.graph) {
                        self.status_message = format!("Preset load error: {}", err);
                        return;
                    }

                    self.session.preset = preset.clone();
                    self.sync_transport_state_from_graph();
                    self.status_message = format!("Preset loaded: {}", preset.name);
                }
                Err(err) => {
                    self.status_message = format!("Preset load error: {}", err);
                }
            }
        }
    }

    fn build_preset(&self) -> Preset {
        let preset_name = if self.session.preset.name.is_empty() {
            self.session.name.clone()
        } else {
            self.session.preset.name.clone()
        };

        Preset {
            name: preset_name,
            graph: self.audio_engine.snapshot_serialized_graph(),
        }
    }

    fn build_session(&self) -> Session {
        Session {
            name: self.session.name.clone(),
            sample_rate: self.audio_engine.sample_rate,
            buffer_size: self.audio_engine.buffer_size,
            preset: self.build_preset(),
            chain: Vec::new(),
            graph: None,
        }
    }

    fn sync_transport_state_from_graph(&mut self) {
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

    fn open_preferences(&mut self) {
        self.show_preferences = true;
        self.preferences_state = Some(PreferencesState::new(
            self.audio_engine.current_host_id(),
            self.audio_engine.sample_rate as u32,
            self.audio_engine.buffer_size,
            self.custom_plugin_paths.clone(),
            self.audio_engine.input_channel,
            self.audio_engine.output_channels,
        ));
    }

    fn perform_undo(&mut self) {
        if let Some(step) = self.undo_manager.pop_undo() {
            let actions: Vec<UndoAction> = step.actions.into_iter().rev().collect();
            self.audio_engine.execute_undo_actions(&actions);
            self.status_message = format!("Undo: {}", step.label);
        }
    }

    fn perform_redo(&mut self) {
        if let Some(step) = self.undo_manager.pop_redo() {
            self.audio_engine.execute_redo_actions(&step.actions);
            self.status_message = format!("Redo: {}", step.label);
        }
    }
}

impl App for ToneDockApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        if self.main_hwnd.is_none() {
            if let Ok(hwnd) = crate::vst_host::editor::extract_hwnd_from_frame(frame) {
                self.main_hwnd = std::ptr::NonNull::new(hwnd);
            }
        }

        TopBottomPanel::top("toolbar")
            .exact_height(58.0)
            .frame(egui::Frame {
                fill: Color32::TRANSPARENT,
                inner_margin: Margin::symmetric(10, 6),
                stroke: Stroke::NONE,
                ..Default::default()
            })
            .show(ctx, |ui| {
                let bar_rect = ui.max_rect();
                ui.painter().rect_filled(
                    bar_rect,
                    CornerRadius::ZERO,
                    Color32::from_rgb(34, 36, 39),
                );
                ui.painter().rect_filled(
                    Rect::from_min_max(bar_rect.min, pos2(bar_rect.max.x, bar_rect.min.y + 14.0)),
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 14),
                );
                for i in 0..18 {
                    let y = bar_rect.top() + i as f32 * 3.0;
                    ui.painter().line_segment(
                        [pos2(bar_rect.left(), y), pos2(bar_rect.right(), y)],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 4)),
                    );
                }
                ui.painter().line_segment(
                    [
                        pos2(bar_rect.left(), bar_rect.bottom() - 1.0),
                        pos2(bar_rect.right(), bar_rect.bottom() - 1.0),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;

                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ToneDock")
                                .size(19.0)
                                .color(crate::ui::theme::ACCENT)
                                .strong(),
                        );
                        ui.label(
                            RichText::new("Digital Guitar Rack")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_HINT),
                        );
                    });

                    ui.add_space(8.0);

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("FILE")
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                            if ui.button("Save Session").clicked() {
                                self.save_session();
                            }
                            if ui.button("Load Session").clicked() {
                                self.load_session();
                            }
                            if ui.button("Save Preset").clicked() {
                                self.save_preset();
                            }
                            if ui.button("Load Preset").clicked() {
                                self.load_preset();
                            }
                            if ui.button("Settings").clicked() {
                                self.open_preferences();
                            }
                        });
                    });

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("ENGINE")
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                            let running = self.audio_engine.is_running();
                            let label = if running { "Stop Audio" } else { "Start Audio" };
                            if ui
                                .add_sized(
                                    [92.0, 28.0],
                                    Button::new(label).fill(if running {
                                        Color32::from_rgb(88, 42, 42)
                                    } else {
                                        Color32::from_rgb(48, 78, 56)
                                    }),
                                )
                                .clicked()
                            {
                                if running {
                                    self.audio_engine.stop();
                                } else {
                                    self.start_audio();
                                }
                            }

                            ui.label(
                                RichText::new("Master")
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                            let mut vol = self.master_volume;
                            ui.add_sized(
                                [88.0, 22.0],
                                egui::Slider::new(&mut vol, 0.0..=1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                            if (vol - self.master_volume).abs() > 0.001 {
                                self.master_volume = vol;
                                *self.audio_engine.master_volume.lock() = vol;
                            }

                            ui.label(
                                RichText::new("Gain")
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                            let mut gain = self.input_gain;
                            ui.add_sized(
                                [58.0, 24.0],
                                egui::DragValue::new(&mut gain).speed(0.01).range(0.0..=4.0),
                            );
                            if (gain - self.input_gain).abs() > 0.001 {
                                self.input_gain = gain;
                                *self.audio_engine.input_gain.lock() = gain;
                            }
                        });
                    });

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("VIEW")
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                            let view_label = match self.view_mode {
                                ViewMode::Rack => "Node View",
                                ViewMode::NodeEditor => "Rack View",
                            };
                            if ui.button(view_label).clicked() {
                                self.view_mode = match self.view_mode {
                                    ViewMode::Rack => ViewMode::NodeEditor,
                                    ViewMode::NodeEditor => ViewMode::Rack,
                                };
                            }
                            if ui.button("About").clicked() {
                                self.show_about = true;
                            }
                        });
                    });

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("EDIT")
                                    .size(9.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                            let can_undo = self.undo_manager.can_undo();
                            let can_redo = self.undo_manager.can_redo();
                            ui.add_enabled_ui(can_undo, |ui| {
                                if ui
                                    .add_sized([42.0, 28.0], Button::new("\u{21a9}"))
                                    .clicked()
                                {
                                    self.perform_undo();
                                }
                            });
                            ui.add_enabled_ui(can_redo, |ui| {
                                if ui
                                    .add_sized([42.0, 28.0], Button::new("\u{21aa}"))
                                    .clicked()
                                {
                                    self.perform_redo();
                                }
                            });
                        });
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui_section_frame().show(ui, |ui| {
                            ui.label(
                                RichText::new(&self.status_message)
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        });
                    });
                });
            });

        TopBottomPanel::bottom("transport")
            .exact_height(56.0)
            .frame(egui::Frame {
                fill: Color32::TRANSPARENT,
                inner_margin: Margin::symmetric(10, 6),
                stroke: Stroke::NONE,
                ..Default::default()
            })
            .show(ctx, |ui| {
                let bar_rect = ui.max_rect();
                ui.painter().rect_filled(
                    bar_rect,
                    CornerRadius::ZERO,
                    Color32::from_rgb(36, 38, 40),
                );
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        pos2(bar_rect.left(), bar_rect.bottom() - 18.0),
                        bar_rect.max,
                    ),
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 44),
                );
                for i in 0..18 {
                    let y = bar_rect.top() + i as f32 * 3.0;
                    ui.painter().line_segment(
                        [pos2(bar_rect.left(), y), pos2(bar_rect.right(), y)],
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 4)),
                    );
                }
                ui.painter().line_segment(
                    [
                        pos2(bar_rect.left(), bar_rect.top()),
                        pos2(bar_rect.right(), bar_rect.top()),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("METRONOME")
                                    .size(9.0)
                                    .color(crate::ui::theme::ACCENT),
                            );

                            if crate::ui::controls::draw_toggle(
                                ui,
                                "",
                                self.metronome_enabled,
                                14.0,
                            ) {
                                self.metronome_enabled = !self.metronome_enabled;
                                if self.metronome_node_id.is_none() && self.metronome_enabled {
                                    self.metronome_node_id =
                                        Some(self.audio_engine.add_metronome_node());
                                }
                                if let Some(id) = self.metronome_node_id {
                                    self.audio_engine.graph_set_state(
                                        id,
                                        NodeInternalState::Metronome(MetronomeNodeState {
                                            bpm: self.metronome_bpm,
                                            volume: self.metronome_volume,
                                        }),
                                    );
                                    self.audio_engine
                                        .graph_set_enabled(id, self.metronome_enabled);
                                    self.audio_engine.graph_commit_topology();
                                }
                            }

                            ui.label("BPM");
                            let mut bpm = self.metronome_bpm;
                            ui.add_sized(
                                [56.0, 24.0],
                                egui::DragValue::new(&mut bpm)
                                    .speed(1.0)
                                    .range(40.0..=300.0),
                            );
                            if (bpm - self.metronome_bpm).abs() > 0.01 {
                                self.metronome_bpm = bpm;
                                if let Some(id) = self.metronome_node_id {
                                    self.audio_engine.graph_set_state(
                                        id,
                                        NodeInternalState::Metronome(MetronomeNodeState {
                                            bpm,
                                            volume: self.metronome_volume,
                                        }),
                                    );
                                }
                            }

                            ui.label("Vol");
                            let mut vol = self.metronome_volume;
                            ui.add_sized([74.0, 22.0], egui::Slider::new(&mut vol, 0.0..=1.0));
                            if (vol - self.metronome_volume).abs() > 0.001 {
                                self.metronome_volume = vol;
                                if let Some(id) = self.metronome_node_id {
                                    self.audio_engine.graph_set_state(
                                        id,
                                        NodeInternalState::Metronome(MetronomeNodeState {
                                            bpm: self.metronome_bpm,
                                            volume: vol,
                                        }),
                                    );
                                }
                            }
                        });
                    });

                    ui_section_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("LOOPER")
                                    .size(10.0)
                                    .color(crate::ui::theme::ACCENT),
                            );

                            if crate::ui::controls::draw_toggle(ui, "", self.looper_enabled, 14.0) {
                                self.looper_enabled = !self.looper_enabled;
                                if self.looper_node_id.is_none() && self.looper_enabled {
                                    self.looper_node_id = Some(self.audio_engine.add_looper_node());
                                }
                                if let Some(id) = self.looper_node_id {
                                    self.audio_engine.graph_set_state(
                                        id,
                                        NodeInternalState::Looper(LooperNodeState {
                                            enabled: self.looper_enabled,
                                            recording: false,
                                            playing: false,
                                            overdubbing: false,
                                            cleared: false,
                                        }),
                                    );
                                    self.audio_engine.graph_set_enabled(id, self.looper_enabled);
                                    self.audio_engine.graph_commit_topology();
                                }
                                if !self.looper_enabled {
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
                                            }),
                                        );
                                    }
                                }
                            }

                            let rec_fill = if self.looper_recording {
                                Color32::from_rgb(112, 40, 40)
                            } else {
                                crate::ui::theme::SURFACE_CONTAINER_HIGH
                            };
                            if ui
                                .add_sized([48.0, 28.0], Button::new("Rec").fill(rec_fill))
                                .clicked()
                            {
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
                                        }),
                                    );
                                    self.audio_engine.graph_set_enabled(id, true);
                                    self.audio_engine.graph_commit_topology();
                                }
                            }

                            let play_fill = if self.looper_playing {
                                Color32::from_rgb(56, 80, 62)
                            } else {
                                crate::ui::theme::SURFACE_CONTAINER_HIGH
                            };
                            if ui
                                .add_sized([50.0, 28.0], Button::new("Play").fill(play_fill))
                                .clicked()
                            {
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
                                        }),
                                    );
                                    self.audio_engine.graph_set_enabled(id, true);
                                    self.audio_engine.graph_commit_topology();
                                }
                            }

                            let dub_fill = if self.looper_overdubbing {
                                Color32::from_rgb(88, 72, 30)
                            } else {
                                crate::ui::theme::SURFACE_CONTAINER_HIGH
                            };
                            if ui
                                .add_sized([68.0, 28.0], Button::new("Overdub").fill(dub_fill))
                                .clicked()
                            {
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
                                        }),
                                    );
                                }
                            }

                            if ui.add_sized([52.0, 28.0], Button::new("Clear")).clicked() {
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
                                        }),
                                    );
                                }
                            }
                        });
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui_section_frame().show(ui, |ui| {
                            if let Some(id) = self.looper_node_id {
                                let guard = self.audio_engine.graph.load();
                                let loop_samples = guard.looper_loop_length(id);
                                drop(guard);
                                if loop_samples > 0 {
                                    let sr = self.audio_engine.sample_rate;
                                    let secs = loop_samples as f64 / sr;
                                    ui.label(
                                        RichText::new(format!("Loop {:.1}s", secs))
                                            .size(10.0)
                                            .color(crate::ui::theme::TEXT_SECONDARY),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new("Transport idle")
                                            .size(10.0)
                                            .color(crate::ui::theme::TEXT_HINT),
                                    );
                                }
                            } else {
                                ui.label(
                                    RichText::new("Transport idle")
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_HINT),
                                );
                            }
                        });
                    });
                });
            });

        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(crate::ui::theme::BG_DARK)
                    .stroke(Stroke::NONE)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| match self.view_mode {
                ViewMode::Rack => self.show_rack_view(ui),
                ViewMode::NodeEditor => self.show_node_editor(ui),
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.perform_undo();
        }
        if ctx.input(|i| {
            (i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
                || (i.key_pressed(egui::Key::Y) && i.modifiers.ctrl)
        }) {
            self.perform_redo();
        }

        if self.show_preferences {
            if let Some(ref mut state) = self.preferences_state {
                let pref_result =
                    crate::ui::preferences::show_preferences(ctx, state, &self.available_plugins);
                match pref_result {
                    PreferencesResult::None => {}
                    PreferencesResult::AudioApply {
                        host_id,
                        input_name,
                        output_name,
                        sample_rate,
                        buffer_size,
                        input_ch,
                        output_ch,
                    } => {
                        self.show_preferences = false;
                        if let Err(e) = self.audio_engine.restart_with_config(
                            host_id,
                            input_name.as_deref(),
                            output_name.as_deref(),
                            sample_rate,
                            buffer_size,
                            input_ch,
                            output_ch,
                        ) {
                            self.status_message = format!("Audio restart error: {}", e);
                            log::error!("Audio restart failed: {}", e);
                        } else {
                            self.status_message = format!(
                                "Audio: {}Hz, buffer {}",
                                self.audio_engine.sample_rate as u32, self.audio_engine.buffer_size,
                            );
                        }
                        self.preferences_state = None;
                    }
                    PreferencesResult::AudioCancel => {
                        self.show_preferences = false;
                        self.preferences_state = None;
                    }
                    PreferencesResult::RescanPlugins => {
                        self.custom_plugin_paths = state.custom_plugin_paths.clone();
                        self.scan_plugins_with_custom_paths();
                        if let Some(ref mut s) = self.preferences_state {
                            s.scan_status =
                                format!("Found {} plugins", self.available_plugins.len());
                        }
                    }
                    PreferencesResult::AddPluginPath(path) => {
                        if !self.custom_plugin_paths.contains(&path) {
                            self.custom_plugin_paths.push(path.clone());
                        }
                        if let Some(ref mut s) = self.preferences_state {
                            s.custom_plugin_paths = self.custom_plugin_paths.clone();
                        }
                        let mut scanner = crate::vst_host::scanner::PluginScanner::new();
                        scanner.add_path(path);
                        let plugins = scanner.scan();
                        if !plugins.is_empty() {
                            let mut seen: std::collections::HashSet<std::path::PathBuf> = self
                                .available_plugins
                                .iter()
                                .map(|p| p.path.clone())
                                .collect();
                            let new_count = plugins.len();
                            for p in plugins {
                                if seen.insert(p.path.clone()) {
                                    self.available_plugins.push(p);
                                }
                            }
                            self.status_message =
                                format!("Added {} plugins from custom path", new_count);
                        } else {
                            self.status_message = "No plugins found in selected path".into();
                        }
                        if let Some(ref mut s) = self.preferences_state {
                            s.scan_status =
                                format!("Found {} plugins", self.available_plugins.len());
                        }
                    }
                }
            }
        }

        if self.show_about {
            let mut open = self.show_about;
            Window::new("About ToneDock")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("ToneDock")
                                .size(24.0)
                                .color(crate::ui::theme::ACCENT)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        ui.label("v0.1.0");
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("A guitar practice VST3 host application")
                                .size(11.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("GPL-3.0 License")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                        ui.add_space(12.0);
                        ui.label(format!(
                            "Audio: {:.0} Hz / {} buffer",
                            self.audio_engine.sample_rate, self.audio_engine.buffer_size
                        ));
                        ui.add_space(4.0);
                        ui.label(format!("Plugins scanned: {}", self.available_plugins.len()));
                        ui.add_space(4.0);
                        ui.label(format!(
                            "Chain slots: {}",
                            self.audio_engine.chain.lock().slots.len()
                        ));
                    });
                });
            self.show_about = open;
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.audio_engine.stop();
    }
}

impl ToneDockApp {
    fn show_rack_view(&mut self, ui: &mut Ui) {
        let screen_size = ui.ctx().screen_rect().size();
        let side_width = 280.0;
        crate::ui::theme::paint_panel_texture(ui.painter(), ui.max_rect());
        crate::ui::theme::paint_rack_bay(ui.painter(), ui.max_rect().shrink2(vec2(12.0, 12.0)));

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(screen_size.x - side_width - 36.0);

                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(12, 12, 16, 205))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("DIGITAL RACK")
                                .size(12.0)
                                .strong()
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{} plugins available", self.available_plugins.len()))
                                    .size(10.0)
                                    .color(crate::ui::theme::TEXT_HINT),
                            );
                        });
                    });
                    ui.add_space(10.0);

                    {
                    let mut chain = self.audio_engine.chain.lock();
                    let sr = self.audio_engine.sample_rate;
                    let bs = self.audio_engine.buffer_size as i32;

                    let available = self.available_plugins.clone();
                    let chain_slots = &mut chain.slots;

                    let commands = self.rack_view.show(ui, chain_slots, &available);

                    let mut deferred_editor_cmds: Vec<crate::ui::rack_view::RackCommand> =
                        Vec::new();

                    for cmd in commands {
                        match cmd {
                            crate::ui::rack_view::RackCommand::Select(idx) => {
                                self.selected_chain_slot = Some(idx);
                            }
                            crate::ui::rack_view::RackCommand::Add(plugin_idx) => {
                                log::info!("Add command received: plugin_idx={}", plugin_idx);
                                if let Some(info) = available.get(plugin_idx).cloned() {
                                    log::info!(
                                        "Loading plugin: {} from {:?}",
                                        info.name,
                                        info.path
                                    );
                                    match chain.add_plugin(&info, sr, bs) {
                                        Ok(()) => {
                                            log::info!("Plugin loaded successfully: {}", info.name);
                                            self.status_message = format!("Loaded: {}", info.name);
                                        }
                                        Err(e) => {
                                            log::error!("Load error for {}: {}", info.name, e);
                                            self.status_message = format!("Load error: {}", e);
                                        }
                                    }
                                } else {
                                    log::warn!(
                                        "plugin_idx {} out of bounds (available: {})",
                                        plugin_idx,
                                        available.len()
                                    );
                                }
                            }
                            crate::ui::rack_view::RackCommand::Remove(idx) => {
                                chain.remove_plugin(idx);
                            }
                            crate::ui::rack_view::RackCommand::Move(from, to) => {
                                chain.move_plugin(from, to);
                            }
                            crate::ui::rack_view::RackCommand::ToggleBypass(idx) => {
                                if let Some(slot) = chain.slots.get_mut(idx) {
                                    slot.bypassed = !slot.bypassed;
                                }
                            }
                            crate::ui::rack_view::RackCommand::ToggleEnable(idx) => {
                                if let Some(slot) = chain.slots.get_mut(idx) {
                                    slot.enabled = !slot.enabled;
                                }
                            }
                            crate::ui::rack_view::RackCommand::OpenEditor(_)
                            | crate::ui::rack_view::RackCommand::CloseEditor(_) => {
                                deferred_editor_cmds.push(cmd);
                            }
                        }
                    }

                    drop(chain);

                    for cmd in deferred_editor_cmds {
                        match cmd {
                            crate::ui::rack_view::RackCommand::OpenEditor(idx) => {
                                let chain = self.audio_engine.chain.lock();
                                let ec = chain
                                    .slots
                                    .get(idx)
                                    .and_then(|s| s.instance.as_ref())
                                    .and_then(|p| p.edit_controller().cloned());
                                let name = chain.slots.get(idx).map(|s| s.info.name.clone());
                                drop(chain);

                                if let (Some(ec), Some(name)) = (ec, name) {
                                    let mut chain = self.audio_engine.chain.lock();
                                    if let Some(slot) = chain.slots.get_mut(idx) {
                                        match slot.editor.open_separate_window(
                                            &ec,
                                            &name,
                                            self.main_hwnd.map(|h| h.as_ptr()),
                                        ) {
                                            Ok(()) => {
                                                self.status_message =
                                                    format!("Opened editor: {}", name);
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to open editor for '{}': {}",
                                                    name,
                                                    e
                                                );
                                                self.status_message =
                                                    format!("Editor error: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                            crate::ui::rack_view::RackCommand::CloseEditor(idx) => {
                                let mut chain = self.audio_engine.chain.lock();
                                if let Some(slot) = chain.slots.get_mut(idx) {
                                    let name = slot.info.name.clone();
                                    slot.editor.close();
                                    self.status_message = format!("Closed editor: {}", name);
                                }
                            }
                            _ => {}
                        }
                    }
                }});
            });

            ui.vertical(|ui| {
                ui.set_max_width(side_width);

                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(20, 20, 24, 230))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("RACK CONTROL")
                                .size(11.0)
                                .strong()
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(8.0);

                        let (out_l, out_r) = *self.audio_engine.output_level.lock();
                        crate::ui::meters::draw_stereo_meter(
                            ui,
                            "MASTER OUT",
                            out_l,
                            out_r,
                            side_width - 28.0,
                            68.0,
                        );

                        ui.add_space(6.0);

                        let (in_l, _) = *self.audio_engine.input_level.lock();
                        crate::ui::meters::draw_mono_meter(
                            ui,
                            "MONO INPUT",
                            in_l,
                            side_width - 28.0,
                            68.0,
                        );

                        ui.add_space(10.0);

                        if let Some(idx) = self.selected_chain_slot.or_else(|| {
                            if self.audio_engine.chain.lock().slots.len() == 1 {
                                Some(0)
                            } else {
                                None
                            }
                        }) {
                            self.draw_parameter_panel(ui, idx);
                        } else {
                            Frame::group(ui.style())
                                .fill(crate::ui::theme::BG_PANEL)
                                .inner_margin(16.0)
                                .corner_radius(CornerRadius::same(14))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new("No module selected")
                                                .size(13.0)
                                                .color(crate::ui::theme::TEXT_SECONDARY),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(
                                                "Select a rack unit to inspect and tweak parameters",
                                            )
                                            .size(10.0)
                                            .color(crate::ui::theme::TEXT_HINT),
                                        );
                                    });
                                });
                        }
                    });
            });
        });
    }

    fn show_node_editor(&mut self, ui: &mut Ui) {
        let (snaps, conns) = {
            let guard = self.audio_engine.graph.load();
            let snaps: Vec<NodeSnap> = guard
                .nodes()
                .iter()
                .map(|(&id, node)| NodeSnap {
                    id,
                    node_type: node.node_type.clone(),
                    enabled: node.enabled,
                    bypassed: node.bypassed,
                    pos: node.position,
                    inputs: node.input_ports.clone(),
                    outputs: node.output_ports.clone(),
                    state: node.internal_state.clone(),
                })
                .collect();
            let conns: Vec<Connection> = guard.connections().to_vec();
            (snaps, conns)
        };

        let side_width = 280.0;
        let screen_size = ui.ctx().screen_rect().size();
        let full_w = screen_size.x - side_width - 30.0;
        crate::ui::theme::paint_panel_texture(ui.painter(), ui.max_rect());

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(full_w);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(10, 10, 14, 220))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("SIGNAL FLOW")
                                    .size(12.0)
                                    .strong()
                                    .color(crate::ui::theme::ACCENT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new("Hardware-style routing canvas")
                                        .size(10.0)
                                        .color(crate::ui::theme::TEXT_HINT),
                                );
                            });
                        });
                        ui.add_space(8.0);
                        let cmds =
                            self.node_editor
                                .show(ui, &snaps, &conns, &self.available_plugins);
                        self.process_editor_commands(cmds);
                    });
            });

            ui.vertical(|ui| {
                ui.set_max_width(side_width);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 18, 24, 235))
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                    ))
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("NODE INSPECTOR")
                                .size(11.0)
                                .strong()
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(8.0);

                        let (out_l, out_r) = *self.audio_engine.output_level.lock();
                        crate::ui::meters::draw_stereo_meter(
                            ui,
                            "OUTPUT",
                            out_l,
                            out_r,
                            side_width - 28.0,
                            48.0,
                        );
                        ui.add_space(4.0);
                        let (in_l, _) = *self.audio_engine.input_level.lock();
                        crate::ui::meters::draw_mono_meter(
                            ui,
                            "MONO INPUT",
                            in_l,
                            side_width - 28.0,
                            48.0,
                        );

                        ui.add_space(10.0);

                        if let Some(sel_id) = self.node_editor.selected_node() {
                            self.draw_vst_parameter_panel(ui, sel_id);
                        } else {
                            ui.label(
                                RichText::new("Select a node to inspect")
                                    .size(11.0)
                                    .color(crate::ui::theme::TEXT_SECONDARY),
                            );
                        }
                    });
            });
        });
    }

    fn process_editor_commands(&mut self, cmds: Vec<EdCmd>) {
        let mut undo_actions: Vec<UndoAction> = Vec::new();
        let mut is_continuous = false;

        for cmd in cmds {
            match cmd {
                EdCmd::AddNode(node_type, pos) => {
                    let id =
                        self.audio_engine
                            .add_node_with_position(node_type.clone(), pos.0, pos.1);
                    undo_actions.push(UndoAction::AddedNode {
                        node_id: id,
                        node_type,
                        position: pos,
                    });
                    self.status_message = format!("Added node {:?}", id);
                }
                EdCmd::AddVstNode {
                    plugin_path,
                    plugin_name,
                    pos,
                } => {
                    let node_type = NodeType::VstPlugin {
                        plugin_path: plugin_path.to_string_lossy().into_owned(),
                        plugin_name: plugin_name.clone(),
                    };
                    let id =
                        self.audio_engine
                            .add_node_with_position(node_type.clone(), pos.0, pos.1);
                    undo_actions.push(UndoAction::AddedNode {
                        node_id: id,
                        node_type,
                        position: pos,
                    });
                    match self.audio_engine.load_vst_plugin_to_node(
                        id,
                        &crate::vst_host::scanner::PluginInfo {
                            path: plugin_path.clone(),
                            name: plugin_name.clone(),
                            category: String::new(),
                            vendor: String::new(),
                        },
                    ) {
                        Ok(()) => {
                            self.status_message = format!("Loaded VST: {}", plugin_name);
                        }
                        Err(e) => {
                            self.status_message = format!("VST load error: {}", e);
                            log::error!("Failed to load VST plugin '{}': {}", plugin_name, e);
                        }
                    }
                }
                EdCmd::RemoveNode(id) => {
                    {
                        let guard = self.audio_engine.graph.load();
                        if let Some(node) = guard.get_node(id) {
                            let connections: Vec<Connection> = guard
                                .connections()
                                .iter()
                                .filter(|c| c.source_node == id || c.target_node == id)
                                .cloned()
                                .collect();
                            undo_actions.push(UndoAction::RemovedNode {
                                node_id: id,
                                node_type: node.node_type.clone(),
                                position: node.position,
                                enabled: node.enabled,
                                bypassed: node.bypassed,
                                state: node.internal_state.clone(),
                                connections,
                            });
                        }
                    }
                    self.audio_engine.graph_remove_node(id);
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = format!("Removed node {:?}", id);
                }
                EdCmd::Connect(conn) => {
                    undo_actions.push(UndoAction::Connected(conn.clone()));
                    self.audio_engine.graph_connect(conn);
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = "Connected".into();
                }
                EdCmd::Disconnect(src_node, src_port, tgt_node, tgt_port) => {
                    let conn = Connection {
                        source_node: src_node,
                        source_port: src_port,
                        target_node: tgt_node,
                        target_port: tgt_port,
                    };
                    undo_actions.push(UndoAction::Disconnected(conn));
                    self.audio_engine
                        .graph_disconnect((src_node, src_port), (tgt_node, tgt_port));
                    self.audio_engine.apply_commands_to_staging();
                    self.status_message = "Disconnected".into();
                }
                EdCmd::SetPos(id, x, y) => {
                    let old_pos = {
                        let guard = self.audio_engine.graph.load();
                        guard.get_node(id).map(|n| n.position).unwrap_or((0.0, 0.0))
                    };
                    undo_actions.push(UndoAction::MovedNode {
                        node_id: id,
                        old_pos,
                        new_pos: (x, y),
                    });
                    self.audio_engine.send_command(
                        crate::audio::graph_command::GraphCommand::SetNodePosition(id, x, y),
                    );
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::SetState(id, state) => {
                    let old_state = {
                        let guard = self.audio_engine.graph.load();
                        guard
                            .get_node(id)
                            .map(|n| n.internal_state.clone())
                            .unwrap_or(NodeInternalState::None)
                    };
                    if std::mem::discriminant(&old_state) == std::mem::discriminant(&state) {
                        is_continuous = true;
                    }
                    undo_actions.push(UndoAction::ChangedState {
                        node_id: id,
                        old_state,
                        new_state: state.clone(),
                    });
                    self.audio_engine.graph_set_state(id, state);
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::ToggleBypass(id) => {
                    let old_bypassed = {
                        let guard = self.audio_engine.graph.load();
                        guard.get_node(id).map(|n| n.bypassed).unwrap_or(false)
                    };
                    undo_actions.push(UndoAction::ChangedBypass {
                        node_id: id,
                        old_bypassed,
                        new_bypassed: !old_bypassed,
                    });
                    self.audio_engine.graph_set_bypassed(id, !old_bypassed);
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::DuplicateNode(id) => {
                    let guard = self.audio_engine.graph.load();
                    if let Some(node) = guard.get_node(id) {
                        let node_type = node.node_type.clone();
                        let state = node.internal_state.clone();
                        let (ox, oy) = node.position;
                        drop(guard);
                        let new_id = self.audio_engine.add_node_with_position(
                            node_type.clone(),
                            ox + 50.0,
                            oy + 50.0,
                        );
                        self.audio_engine.graph_set_state(new_id, state.clone());
                        self.audio_engine.graph_commit_topology();
                        self.audio_engine.apply_commands_to_staging();
                        undo_actions.push(UndoAction::AddedNode {
                            node_id: new_id,
                            node_type,
                            position: (ox + 50.0, oy + 50.0),
                        });
                        self.node_editor.set_selection(Some(new_id));
                        self.status_message = format!("Duplicated node {:?}", new_id);
                    }
                }
                EdCmd::SetVstParameter {
                    node_id,
                    param_index,
                    value,
                } => {
                    self.audio_engine
                        .set_vst_node_parameter(node_id, param_index, value);
                }
                EdCmd::Commit => {
                    self.audio_engine.graph_commit_topology();
                    self.audio_engine.apply_commands_to_staging();
                }
                EdCmd::ApplyTemplate(template_name, pos) => {
                    self.apply_template(&template_name, pos);
                }
            }
        }

        if !undo_actions.is_empty() {
            let label = if is_continuous {
                "Change Parameter".into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::AddedNode { .. }))
            {
                "Add Node".into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::RemovedNode { .. }))
            {
                "Remove Node".into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::Connected(_)))
            {
                "Connect".into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::Disconnected(_)))
            {
                "Disconnect".into()
            } else if undo_actions
                .iter()
                .any(|a| matches!(a, UndoAction::MovedNode { .. }))
            {
                "Move Node".into()
            } else {
                "Edit".into()
            };

            self.undo_manager.push(UndoStep {
                label,
                actions: undo_actions,
                is_continuous,
            });
        }
    }

    fn apply_template(&mut self, name: &str, base_pos: (f32, f32)) {
        match name {
            "wide_stereo_amp" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let pan_l_id = self.audio_engine.add_node_with_position(
                    NodeType::Pan,
                    base_pos.0 - 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    if guard.get_node(pan_l_id).is_some() {
                        let mut staging = (**guard).clone();
                        drop(guard);
                        if let Some(n) = staging.get_node_mut(pan_l_id) {
                            n.internal_state = NodeInternalState::Pan { value: -0.8 };
                        }
                        self.audio_engine.graph.store(Arc::new(staging));
                    }
                }
                let pan_r_id = self.audio_engine.add_node_with_position(
                    NodeType::Pan,
                    base_pos.0 + 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(pan_r_id) {
                        n.internal_state = NodeInternalState::Pan { value: 0.8 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 250.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: pan_l_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: pan_r_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: pan_l_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: pan_r_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = "Template: Wide Stereo Amp applied".into();
            }
            "dry_wet_blend" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let wetdry_id = self.audio_engine.add_node_with_position(
                    NodeType::WetDry,
                    base_pos.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(wetdry_id) {
                        n.internal_state = NodeInternalState::WetDry { mix: 0.5 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: wetdry_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: wetdry_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = "Template: Dry/Wet Blend applied".into();
            }
            "mono_stereo_reverb" => {
                let converter_id = self.audio_engine.add_node_with_position(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Stereo,
                    },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: converter_id,
                    source_port: PortId(0),
                    target_node: self.audio_engine.output_node_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = "Template: Mono→Stereo Reverb applied".into();
            }
            "send_return_reverb" => {
                let send_id = self.audio_engine.add_node_with_position(
                    NodeType::SendBus { bus_id: 1 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let return_id = self.audio_engine.add_node_with_position(
                    NodeType::ReturnBus { bus_id: 1 },
                    base_pos.0 + 120.0,
                    base_pos.1 + 200.0,
                );
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 350.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: send_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: send_id,
                    source_port: PortId(1),
                    target_node: return_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: return_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = "Template: Send/Return Reverb applied".into();
            }
            "parallel_chain" => {
                let splitter_id = self.audio_engine.add_node_with_position(
                    NodeType::Splitter { outputs: 2 },
                    base_pos.0,
                    base_pos.1 + 50.0,
                );
                let gain_a_id = self.audio_engine.add_node_with_position(
                    NodeType::Gain,
                    base_pos.0 - 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(gain_a_id) {
                        n.internal_state = NodeInternalState::Gain { value: 0.8 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let gain_b_id = self.audio_engine.add_node_with_position(
                    NodeType::Gain,
                    base_pos.0 + 80.0,
                    base_pos.1 + 150.0,
                );
                {
                    let guard = self.audio_engine.graph.load();
                    let mut staging = (**guard).clone();
                    drop(guard);
                    if let Some(n) = staging.get_node_mut(gain_b_id) {
                        n.internal_state = NodeInternalState::Gain { value: 0.6 };
                    }
                    self.audio_engine.graph.store(Arc::new(staging));
                }
                let mixer_id = self.audio_engine.add_node_with_position(
                    NodeType::Mixer { inputs: 2 },
                    base_pos.0,
                    base_pos.1 + 250.0,
                );

                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(0),
                    target_node: gain_a_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: splitter_id,
                    source_port: PortId(1),
                    target_node: gain_b_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: gain_a_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(0),
                });
                self.audio_engine.graph_connect(Connection {
                    source_node: gain_b_id,
                    source_port: PortId(0),
                    target_node: mixer_id,
                    target_port: PortId(1),
                });
                self.audio_engine.graph_commit_topology();
                self.audio_engine.apply_commands_to_staging();
                self.status_message = "Template: Parallel Chain applied".into();
            }
            _ => {
                self.status_message = format!("Unknown template: {}", name);
            }
        }
    }
}

impl ToneDockApp {
    fn draw_parameter_panel(&mut self, ui: &mut Ui, slot_index: usize) {
        let slot_name = {
            let chain = self.audio_engine.chain.lock();
            chain
                .slots
                .get(slot_index)
                .map(|s| s.info.name.clone())
                .unwrap_or_else(|| "PARAMETERS".into())
        };
        Frame::group(ui.style())
            .fill(crate::ui::theme::BG_PANEL)
            .inner_margin(12.0)
            .corner_radius(CornerRadius::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("MODULE EDIT")
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.label(
                    RichText::new(slot_name)
                        .size(14.0)
                        .color(crate::ui::theme::TEXT_PRIMARY),
                );
                ui.add_space(4.0);

                let param_infos = {
                    let chain = self.audio_engine.chain.lock();
                    chain.get_parameter_info(slot_index)
                };

                let params_per_row = 3;
                let knob_size = 50.0;

                for chunk in param_infos.chunks(params_per_row) {
                    ui.horizontal_wrapped(|ui| {
                        for (j, _param) in chunk.iter().enumerate() {
                            let param_idx = (chunk.as_ptr() as usize
                                - param_infos.as_ptr() as usize)
                                / std::mem::size_of::<crate::audio::chain::ParamInfo>()
                                + j;
                            let mut value = {
                                let chain = self.audio_engine.chain.lock();
                                chain.get_parameter(slot_index, param_idx).unwrap_or(0.0)
                            };

                            ui.vertical(|ui| {
                                crate::ui::controls::draw_knob(
                                    ui,
                                    &mut value,
                                    &chunk[j].name,
                                    0.0,
                                    1.0,
                                    knob_size,
                                );
                            });

                            {
                                let mut chain = self.audio_engine.chain.lock();
                                chain.set_parameter(slot_index, param_idx, value);
                            }
                        }
                    });
                }

                if param_infos.is_empty() {
                    ui.label(
                        RichText::new("No exposed parameters")
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
            });
    }

    fn draw_vst_parameter_panel(&mut self, ui: &mut Ui, node_id: NodeId) {
        let node_name = {
            let guard = self.audio_engine.graph.load();
            guard
                .get_node(node_id)
                .map(|n| match &n.node_type {
                    NodeType::VstPlugin { plugin_name, .. } => plugin_name.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default()
        };

        if node_name.is_empty() {
            return;
        }

        let has_plugin = {
            let guard = self.audio_engine.graph.load();
            guard
                .get_node(node_id)
                .map(|n| n.plugin_instance.lock().is_some())
                .unwrap_or(false)
        };

        if !has_plugin {
            Frame::group(ui.style())
                .fill(crate::ui::theme::BG_PANEL)
                .inner_margin(12.0)
                .corner_radius(CornerRadius::same(14))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(&node_name)
                                .size(11.0)
                                .color(crate::ui::theme::ACCENT),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Plugin not loaded")
                                .size(10.0)
                                .color(crate::ui::theme::TEXT_SECONDARY),
                        );
                    });
                });
            return;
        }

        let param_infos = self.audio_engine.get_vst_node_parameters(node_id);

        Frame::group(ui.style())
            .fill(crate::ui::theme::BG_PANEL)
            .inner_margin(12.0)
            .corner_radius(CornerRadius::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("PLUGIN EDITOR")
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.label(
                    RichText::new(&node_name)
                        .size(13.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.add_space(4.0);

                let knob_size = 44.0;
                let params_per_row = 3;

                for chunk in param_infos.chunks(params_per_row) {
                    ui.horizontal_wrapped(|ui| {
                        for (j, _param) in chunk.iter().enumerate() {
                            let param_idx = (chunk.as_ptr() as usize
                                - param_infos.as_ptr() as usize)
                                / std::mem::size_of::<crate::audio::chain::ParamInfo>()
                                + j;
                            let mut value = self
                                .audio_engine
                                .get_vst_node_parameter_value(node_id, param_idx);

                            ui.vertical(|ui| {
                                crate::ui::controls::draw_knob(
                                    ui,
                                    &mut value,
                                    &chunk[j].name,
                                    0.0,
                                    1.0,
                                    knob_size,
                                );
                            });

                            self.audio_engine
                                .set_vst_node_parameter(node_id, param_idx, value);
                        }
                    });
                }

                if param_infos.is_empty() {
                    ui.label(
                        RichText::new("No exposed parameters")
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
            });
    }
}
