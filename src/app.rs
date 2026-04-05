use eframe::App;
use egui::*;

use crate::audio::engine::AudioEngine;
use crate::session::Session;
use crate::ui::preferences::{PreferencesResult, PreferencesState};
use crate::ui::rack_view::RackView;
use crate::vst_host::scanner::PluginInfo;

pub struct ToneDockApp {
    audio_engine: AudioEngine,
    rack_view: RackView,
    available_plugins: Vec<PluginInfo>,
    custom_plugin_paths: Vec<std::path::PathBuf>,
    session: Session,

    metronome_enabled: bool,
    metronome_bpm: f64,
    metronome_volume: f32,

    looper_enabled: bool,
    looper_recording: bool,
    looper_playing: bool,
    looper_overdubbing: bool,

    selected_chain_slot: Option<usize>,
    show_about: bool,
    status_message: String,

    show_preferences: bool,
    preferences_state: Option<PreferencesState>,

    master_volume: f32,
    input_gain: f32,
}

impl ToneDockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::ui::theme::apply_fonts(&cc.egui_ctx);

        let audio_engine = AudioEngine::new().unwrap_or_else(|e| {
            log::error!("Failed to create audio engine: {}", e);
            panic!("Audio engine init failed: {}", e);
        });

        let mut app = Self {
            audio_engine,
            rack_view: RackView::new(),
            available_plugins: Vec::new(),
            custom_plugin_paths: Vec::new(),
            session: Session::default(),
            metronome_enabled: false,
            metronome_bpm: 120.0,
            metronome_volume: 0.5,
            looper_enabled: false,
            looper_recording: false,
            looper_playing: false,
            looper_overdubbing: false,
            selected_chain_slot: None,
            show_about: false,
            status_message: "Ready".into(),
            show_preferences: false,
            preferences_state: None,
            master_volume: 0.8,
            input_gain: 1.0,
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
            let sr = self.audio_engine.sample_rate;
            self.audio_engine.metronome.lock().set_sample_rate(sr);
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

    fn load_session(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ToneDock Session", &["tonedock.json"])
            .pick_file()
        {
            match Session::load_from_file(&path) {
                Ok(session) => {
                    self.status_message = format!("Loaded: {}", session.name);
                    self.session = session;
                }
                Err(e) => {
                    self.status_message = format!("Load error: {}", e);
                }
            }
        }
    }

    fn build_session(&self) -> Session {
        let chain = self.audio_engine.chain.lock();
        let chain_slots: Vec<crate::session::ChainSlot> = chain
            .slots
            .iter()
            .map(|s| crate::session::ChainSlot {
                plugin_path: s.info.path.to_string_lossy().into_owned(),
                plugin_name: s.info.name.clone(),
                enabled: s.enabled,
                parameters: Vec::new(),
            })
            .collect();
        Session {
            name: self.session.name.clone(),
            sample_rate: self.audio_engine.sample_rate,
            buffer_size: self.audio_engine.buffer_size,
            chain: chain_slots,
        }
    }

    fn open_preferences(&mut self) {
        self.show_preferences = true;
        self.preferences_state = Some(PreferencesState::new(
            self.audio_engine.current_host_id(),
            self.audio_engine.sample_rate as u32,
            self.audio_engine.buffer_size,
            self.custom_plugin_paths.clone(),
            self.audio_engine.input_channels,
            self.audio_engine.output_channels,
        ));
    }
}

impl App for ToneDockApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals {
            dark_mode: true,
            override_text_color: Some(crate::ui::theme::TEXT_PRIMARY),
            ..Visuals::dark()
        });

        let screen_size = ctx.screen_rect().size();

        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("ToneDock")
                        .size(16.0)
                        .color(crate::ui::theme::ACCENT)
                        .strong(),
                );
                ui.separator();

                if ui.button("Save Session").clicked() {
                    self.save_session();
                }
                if ui.button("Load Session").clicked() {
                    self.load_session();
                }
                ui.separator();

                if ui.button("Preferences...").clicked() {
                    self.open_preferences();
                }

                let running = self.audio_engine.is_running();
                let label = if running { "Stop Audio" } else { "Start Audio" };
                if ui.button(label).clicked() {
                    if running {
                        self.audio_engine.stop();
                    } else {
                        self.start_audio();
                    }
                }

                if ui.button("About").clicked() {
                    self.show_about = true;
                }

                ui.separator();

                ui.label("Master:");
                let mut vol = self.master_volume;
                ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false));
                if (vol - self.master_volume).abs() > 0.001 {
                    self.master_volume = vol;
                    *self.audio_engine.master_volume.lock() = vol;
                }

                ui.label("Gain:");
                let mut gain = self.input_gain;
                ui.add(egui::DragValue::new(&mut gain).speed(0.01).range(0.0..=4.0));
                if (gain - self.input_gain).abs() > 0.001 {
                    self.input_gain = gain;
                    *self.audio_engine.input_gain.lock() = gain;
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&self.status_message)
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                });
            });
        });

        TopBottomPanel::bottom("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("METRONOME")
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT),
                );

                if crate::ui::controls::draw_toggle(ui, "", self.metronome_enabled, 14.0) {
                    self.metronome_enabled = !self.metronome_enabled;
                    self.audio_engine.metronome.lock().enabled = self.metronome_enabled;
                }

                ui.label("BPM:");
                let mut bpm = self.metronome_bpm;
                ui.add(
                    egui::DragValue::new(&mut bpm)
                        .speed(1.0)
                        .range(40.0..=300.0),
                );
                if (bpm - self.metronome_bpm).abs() > 0.01 {
                    self.metronome_bpm = bpm;
                    self.audio_engine.metronome.lock().set_bpm(bpm);
                }

                ui.label("Vol:");
                let mut vol = self.metronome_volume;
                ui.add(egui::Slider::new(&mut vol, 0.0..=1.0));
                if (vol - self.metronome_volume).abs() > 0.001 {
                    self.metronome_volume = vol;
                    self.audio_engine.metronome.lock().volume = vol;
                }

                ui.separator();

                ui.label(
                    RichText::new("LOOPER")
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT),
                );

                if crate::ui::controls::draw_toggle(ui, "", self.looper_enabled, 14.0) {
                    self.looper_enabled = !self.looper_enabled;
                    self.audio_engine.looper.lock().enabled = self.looper_enabled;
                    if !self.looper_enabled {
                        self.looper_recording = false;
                        self.looper_playing = false;
                        self.looper_overdubbing = false;
                        self.audio_engine.looper.lock().clear();
                    }
                }

                let rec_color = if self.looper_recording {
                    crate::ui::theme::METER_RED
                } else {
                    crate::ui::theme::TEXT_SECONDARY
                };
                ui.style_mut().visuals.override_text_color = Some(rec_color);
                if ui.button("Rec").clicked() {
                    let mut lpr = self.audio_engine.looper.lock();
                    self.looper_enabled = true;
                    lpr.enabled = true;
                    lpr.toggle_record();
                    self.looper_recording = lpr.recording;
                    self.looper_playing = lpr.playing;
                }
                ui.style_mut().visuals.override_text_color = Some(crate::ui::theme::TEXT_PRIMARY);

                let play_color = if self.looper_playing {
                    crate::ui::theme::METER_GREEN
                } else {
                    crate::ui::theme::TEXT_SECONDARY
                };
                ui.style_mut().visuals.override_text_color = Some(play_color);
                if ui.button("Play").clicked() {
                    let mut lpr = self.audio_engine.looper.lock();
                    self.looper_enabled = true;
                    lpr.enabled = true;
                    lpr.toggle_play();
                    self.looper_playing = lpr.playing;
                    self.looper_recording = lpr.recording;
                }
                ui.style_mut().visuals.override_text_color = Some(crate::ui::theme::TEXT_PRIMARY);

                let od_color = if self.looper_overdubbing {
                    crate::ui::theme::METER_YELLOW
                } else {
                    crate::ui::theme::TEXT_SECONDARY
                };
                ui.style_mut().visuals.override_text_color = Some(od_color);
                if ui.button("Overdub").clicked() {
                    let mut lpr = self.audio_engine.looper.lock();
                    lpr.toggle_overdub();
                    self.looper_overdubbing = lpr.overdubbing;
                }
                ui.style_mut().visuals.override_text_color = Some(crate::ui::theme::TEXT_PRIMARY);

                if ui.button("Clear").clicked() {
                    let mut lpr = self.audio_engine.looper.lock();
                    lpr.clear();
                    self.looper_recording = false;
                    self.looper_playing = false;
                    self.looper_overdubbing = false;
                }

                let loop_samples = self.audio_engine.looper.lock().loop_length_samples();
                if loop_samples > 0 {
                    let sr = self.audio_engine.sample_rate;
                    let secs = loop_samples as f64 / sr;
                    ui.label(
                        RichText::new(format!("{:.1}s", secs))
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
            });
        });

        CentralPanel::default().show(ctx, |ui| {
            let side_width = 240.0;

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(screen_size.x - side_width - 20.0);

                    {
                        let mut chain = self.audio_engine.chain.lock();
                        let sr = self.audio_engine.sample_rate;
                        let bs = self.audio_engine.buffer_size as i32;

                        let available = self.available_plugins.clone();
                        let chain_slots = &mut chain.slots;

                        let commands = self.rack_view.show(ui, chain_slots, &available);

                        for cmd in commands {
                            match cmd {
                                crate::ui::rack_view::RackCommand::Add(plugin_idx) => {
                                    if let Some(info) = available.get(plugin_idx).cloned() {
                                        if let Err(e) = chain.add_plugin(&info, sr, bs) {
                                            self.status_message = format!("Load error: {}", e);
                                        } else {
                                            self.status_message = format!("Loaded: {}", info.name);
                                        }
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
                            }
                        }
                    }
                });

                ui.vertical(|ui| {
                    ui.set_max_width(side_width);

                    let (out_l, out_r) = *self.audio_engine.output_level.lock();
                    crate::ui::meters::draw_stereo_meter(
                        ui, "OUTPUT", out_l, out_r, side_width, 60.0,
                    );

                    ui.add_space(4.0);

                    let (in_l, in_r) = *self.audio_engine.input_level.lock();
                    crate::ui::meters::draw_stereo_meter(ui, "INPUT", in_l, in_r, side_width, 60.0);

                    ui.add_space(8.0);

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
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new("Select a plugin to edit parameters")
                                            .size(11.0)
                                            .color(crate::ui::theme::TEXT_SECONDARY),
                                    );
                                });
                            });
                    }
                });
            });
        });

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
                            let new_sr = self.audio_engine.sample_rate;
                            self.audio_engine.metronome.lock().set_sample_rate(new_sr);
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
    fn draw_parameter_panel(&mut self, ui: &mut Ui, slot_index: usize) {
        Frame::group(ui.style())
            .fill(crate::ui::theme::BG_PANEL)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("PARAMETERS")
                        .size(10.0)
                        .color(crate::ui::theme::ACCENT),
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
                        RichText::new("No parameters")
                            .size(10.0)
                            .color(crate::ui::theme::TEXT_SECONDARY),
                    );
                }
            });
    }
}
