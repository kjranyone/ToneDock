use std::collections::HashMap;

use crate::audio::node::*;
use crate::i18n::I18n;
use egui::*;

use super::{
    DConn, DragParam, EdCmd, NodeEditor, NodeSnap, COL_CONN, COL_CONN_HOVER, NODE_W,
    PARAM_SENSITIVITY,
};

impl NodeEditor {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        nodes: &[NodeSnap],
        conns: &[Connection],
        available_plugins: &[crate::vst_host::scanner::PluginInfo],
        i18n: &I18n,
    ) -> Vec<EdCmd> {
        let mut cmds = Vec::new();

        let rect = ui.available_rect_before_wrap();
        let canvas_id = ui.id().with("_ne_canvas");
        let resp = ui.interact(rect, canvas_id, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        let mouse_screen: Option<Pos2> = ui.input(|i| i.pointer.latest_pos());
        let mouse = mouse_screen.filter(|&p| rect.contains(p));

        let mut vpos: HashMap<NodeId, (f32, f32)> = nodes.iter().map(|n| (n.id, n.pos)).collect();
        if let (Some(did), Some(ms)) = (self.drag_node, mouse_screen) {
            let mw = self.s2w(ms);
            vpos.insert(did, (mw.x - self.drag_off.x, mw.y - self.drag_off.y));
        }

        if let Some(ms) = mouse {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom;
                self.zoom =
                    (self.zoom * if scroll > 0.0 { 1.1 } else { 1.0_f32 / 1.1 }).clamp(0.2, 4.0);
                self.pan = ms.to_vec2() - (ms.to_vec2() - self.pan) * (self.zoom / old);
            }
        }

        if ui.input(|i| i.key_pressed(Key::F)) {
            self.zoom_to_fit(nodes, rect);
        }

        let ptr_down = resp.is_pointer_button_down_on();
        let just_press = ptr_down && !self.ptr_down;
        let just_release = !ptr_down && self.ptr_down;
        self.ptr_down = ptr_down;

        self.hover_conn = mouse.and_then(|ms| self.hit_connection(conns, nodes, &vpos, ms));

        if just_press {
            if let Some(ms) = mouse_screen {
                if let Some((nid, pid, pidx)) = self.hit_out_port(nodes, ms, &vpos) {
                    let from = self.out_spos(vpos[&nid], pidx);
                    self.dconn = Some(DConn {
                        src_node: nid,
                        src_port: pid,
                        from,
                        to: ms,
                    });
                } else if let Some((target_nid, target_pid)) = self.hit_in_port(nodes, ms, &vpos) {
                    if let Some(conn) = conns
                        .iter()
                        .find(|c| c.target_node == target_nid && c.target_port == target_pid)
                    {
                        let src_pidx = nodes
                            .iter()
                            .find(|n| n.id == conn.source_node)
                            .and_then(|n| n.outputs.iter().position(|p| p.id == conn.source_port));
                        if let Some(pidx) = src_pidx {
                            let from = self.out_spos(vpos[&conn.source_node], pidx);
                            cmds.push(EdCmd::Disconnect(
                                conn.source_node,
                                conn.source_port,
                                conn.target_node,
                                conn.target_port,
                            ));
                            cmds.push(EdCmd::Commit);
                            self.dconn = Some(DConn {
                                src_node: conn.source_node,
                                src_port: conn.source_port,
                                from,
                                to: ms,
                            });
                        }
                    }
                } else if let Some(nid) = self.hit_param(nodes, ms, &vpos) {
                    let value = nodes
                        .iter()
                        .find(|n| n.id == nid)
                        .and_then(|n| match &n.state {
                            NodeInternalState::Gain { value } => Some(*value),
                            NodeInternalState::Pan { value } => Some(*value),
                            NodeInternalState::WetDry { mix } => Some(*mix),
                            NodeInternalState::SendBus { send_level } => Some(*send_level),
                            _ => None,
                        })
                        .unwrap_or(0.0);
                    self.drag_param = Some(DragParam {
                        node_id: nid,
                        start_x: ms.x,
                        start_value: value,
                        current_value: value,
                    });
                    self.sel = Some(nid);
                } else if let Some(nid) = self.hit_node(nodes, ms, &vpos) {
                    self.sel = Some(nid);
                    self.drag_node = Some(nid);
                    let np = vpos[&nid];
                    let mw = self.s2w(ms);
                    self.drag_off = Vec2::new(mw.x - np.0, mw.y - np.1);
                } else {
                    self.sel = None;
                }
            }
        }

        if ptr_down {
            if let Some(ref mut dc) = self.dconn {
                if let Some(ms) = mouse_screen {
                    dc.to = ms;
                }
            }
            if let Some(ref mut dp) = self.drag_param {
                if let Some(ms) = mouse_screen {
                    let delta = ms.x - dp.start_x;
                    let is_gain = nodes
                        .iter()
                        .find(|n| n.id == dp.node_id)
                        .map(|n| matches!(n.state, NodeInternalState::Gain { .. }))
                        .unwrap_or(false);
                    let is_wetdry = nodes
                        .iter()
                        .find(|n| n.id == dp.node_id)
                        .map(|n| matches!(n.state, NodeInternalState::WetDry { .. }))
                        .unwrap_or(false);
                    let is_sendbus = nodes
                        .iter()
                        .find(|n| n.id == dp.node_id)
                        .map(|n| matches!(n.state, NodeInternalState::SendBus { .. }))
                        .unwrap_or(false);
                    let new_value = if is_gain {
                        (dp.start_value + delta * PARAM_SENSITIVITY).clamp(0.0, 4.0)
                    } else if is_wetdry || is_sendbus {
                        (dp.start_value + delta * PARAM_SENSITIVITY).clamp(0.0, 1.0)
                    } else {
                        (dp.start_value + delta * PARAM_SENSITIVITY).clamp(-1.0, 1.0)
                    };
                    dp.current_value = new_value;

                    let state = if is_gain {
                        NodeInternalState::Gain { value: new_value }
                    } else if is_wetdry {
                        NodeInternalState::WetDry { mix: new_value }
                    } else if is_sendbus {
                        NodeInternalState::SendBus {
                            send_level: new_value,
                        }
                    } else {
                        NodeInternalState::Pan { value: new_value }
                    };
                    cmds.push(EdCmd::SetState(dp.node_id, state));
                }
            }
        }

        if resp.dragged_by(PointerButton::Primary)
            && self.drag_node.is_none()
            && self.dconn.is_none()
            && self.drag_param.is_none()
        {
            self.pan += resp.drag_delta();
        }
        if resp.dragged_by(PointerButton::Middle) {
            self.pan += resp.drag_delta();
        }

        if just_release {
            if let Some(did) = self.drag_node.take() {
                if let Some(pos) = vpos.get(&did) {
                    cmds.push(EdCmd::SetPos(did, pos.0, pos.1));
                }
            }
            if let Some(dc) = self.dconn.take() {
                if let Some(ms) = mouse_screen {
                    if let Some((tn, tp)) = self.hit_in_port(nodes, ms, &vpos) {
                        if tn != dc.src_node {
                            cmds.push(EdCmd::Connect(Connection {
                                source_node: dc.src_node,
                                source_port: dc.src_port,
                                target_node: tn,
                                target_port: tp,
                            }));
                            cmds.push(EdCmd::Commit);
                        }
                    }
                }
            }
            self.drag_param = None;
        }

        if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            if self.sel.is_some() {
                if let Some(sid) = self.sel {
                    if !matches!(
                        nodes.iter().find(|n| n.id == sid).map(|n| &n.node_type),
                        Some(NodeType::AudioInput) | Some(NodeType::AudioOutput)
                    ) {
                        cmds.push(EdCmd::RemoveNode(sid));
                        cmds.push(EdCmd::Commit);
                        self.sel = None;
                    }
                }
            } else if let Some(ci) = self.hover_conn {
                if let Some(conn) = conns.get(ci) {
                    cmds.push(EdCmd::Disconnect(
                        conn.source_node,
                        conn.source_port,
                        conn.target_node,
                        conn.target_port,
                    ));
                    cmds.push(EdCmd::Commit);
                }
            }
        }

        if ui.input(|i| i.key_pressed(Key::D) && i.modifiers.ctrl) {
            if let Some(sid) = self.sel {
                if !matches!(
                    nodes.iter().find(|n| n.id == sid).map(|n| &n.node_type),
                    Some(NodeType::AudioInput) | Some(NodeType::AudioOutput)
                ) {
                    cmds.push(EdCmd::DuplicateNode(sid));
                }
            }
        }

        if resp.secondary_clicked() {
            if let Some(pos) = ui
                .input(|i| i.pointer.latest_pos())
                .filter(|&p| rect.contains(p))
            {
                self.menu_wpos = self.s2w(pos);
                self.menu_conn = self.hit_connection(conns, nodes, &vpos, pos);
                if let Some(nid) = self.hit_node(nodes, pos, &vpos) {
                    self.sel = Some(nid);
                }
            }
        }

        let sel_id = self.sel;
        let sel_bypassed = self
            .sel
            .and_then(|id| nodes.iter().find(|n| n.id == id).map(|n| n.bypassed));
        let sel_is_io = self.sel.map(|id| {
            nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| matches!(n.node_type, NodeType::AudioInput | NodeType::AudioOutput))
                .unwrap_or(false)
        });
        let mw = self.menu_wpos;
        let menu_conn = self.menu_conn;
        let mut menu_cmds: Vec<EdCmd> = Vec::new();

        resp.context_menu(|ui| {
            if let Some(ci) = menu_conn {
                if let Some(conn) = conns.get(ci) {
                    ui.label(
                        RichText::new(i18n.tr("node.connection"))
                            .strong()
                            .color(crate::ui::theme::ACCENT),
                    );
                    ui.separator();
                    if ui.button(i18n.tr("node.delete_connection")).clicked() {
                        menu_cmds.push(EdCmd::Disconnect(
                            conn.source_node,
                            conn.source_port,
                            conn.target_node,
                            conn.target_port,
                        ));
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                    ui.separator();
                }
            }

            ui.label(
                RichText::new(i18n.tr("node.add_node"))
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button(i18n.tr("node.gain")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Gain, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.pan")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Pan, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.splitter_2out")).clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::Splitter { outputs: 2 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.mixer_2in")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Mixer { inputs: 2 }, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.converter_ms")).clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Stereo,
                    },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.converter_sm")).clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Mono,
                    },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.metronome")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Metronome, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.looper")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Looper, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.wet_dry")).clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::WetDry, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            ui.separator();
            ui.label(
                RichText::new(i18n.tr("node.send_return_buses"))
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button(i18n.tr("node.send_bus_1")).clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::SendBus { bus_id: 1 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.return_bus_1")).clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ReturnBus { bus_id: 1 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            ui.separator();
            ui.label(
                RichText::new(i18n.tr("node.templates"))
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button(i18n.tr("node.wide_stereo_amp")).clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("wide_stereo_amp".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.dry_wet_blend")).clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("dry_wet_blend".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.send_return_reverb")).clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate(
                    "send_return_reverb".into(),
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.parallel_chain")).clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("parallel_chain".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            if !available_plugins.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new(i18n.tr("node.vst_plugins"))
                        .strong()
                        .color(crate::ui::theme::ACCENT),
                );
                ui.separator();
                for plugin_info in available_plugins {
                    if ui.button(&plugin_info.name).clicked() {
                        menu_cmds.push(EdCmd::AddVstNode {
                            plugin_path: plugin_info.path.clone(),
                            plugin_name: plugin_info.name.clone(),
                            pos: (mw.x, mw.y),
                        });
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                }
            }

            if let Some(sid) = sel_id {
                ui.separator();
                ui.label(
                    RichText::new(i18n.tr("node.selected_node"))
                        .color(crate::ui::theme::TEXT_SECONDARY),
                );
                if let Some(bp) = sel_bypassed {
                    let text = if bp {
                        i18n.tr("rack.unbypass")
                    } else {
                        i18n.tr("rack.bypass")
                    };
                    if ui.button(text).clicked() {
                        menu_cmds.push(EdCmd::ToggleBypass(sid));
                        ui.close_menu();
                    }
                }
                if sel_is_io != Some(true) {
                    if ui.button(i18n.tr("node.delete")).clicked() {
                        menu_cmds.push(EdCmd::RemoveNode(sid));
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                    if ui.button(i18n.tr("node.duplicate")).clicked() {
                        menu_cmds.push(EdCmd::DuplicateNode(sid));
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                }
            }

            ui.separator();
            if ui.button(i18n.tr("node.reset_view")).clicked() {
                self.pan = Vec2::new(100.0, 100.0);
                self.zoom = 1.0;
                ui.close_menu();
            }
            if ui.button(i18n.tr("node.fit_all")).clicked() {
                self.zoom_to_fit(nodes, rect);
                ui.close_menu();
            }
        });

        for c in &menu_cmds {
            if let EdCmd::RemoveNode(id) = c {
                if self.sel == Some(*id) {
                    self.sel = None;
                }
            }
        }
        cmds.extend(menu_cmds);

        painter.rect_filled(rect, 0.0, crate::ui::theme::BG_DARK);
        self.paint_grid(&painter, rect);

        for (ci, conn) in conns.iter().enumerate() {
            let sp = vpos.get(&conn.source_node);
            let tp = vpos.get(&conn.target_node);
            if let (Some(sp), Some(tp)) = (sp, tp) {
                let si = nodes
                    .iter()
                    .find(|n| n.id == conn.source_node)
                    .and_then(|n| n.outputs.iter().position(|p| p.id == conn.source_port));
                let ti = nodes
                    .iter()
                    .find(|n| n.id == conn.target_node)
                    .and_then(|n| n.inputs.iter().position(|p| p.id == conn.target_port));
                if let (Some(si), Some(ti)) = (si, ti) {
                    let from = self.out_spos(*sp, si);
                    let to = self.in_spos(*tp, ti);
                    let hovered = self.hover_conn == Some(ci) || self.menu_conn == Some(ci);
                    let color = if hovered { COL_CONN_HOVER } else { COL_CONN };
                    let width = if hovered { 3.5 } else { 2.0 };
                    self.paint_bezier(&painter, from, to, color, width);
                }
            }
        }

        if let Some(ref dc) = self.dconn {
            self.paint_bezier(&painter, dc.from, dc.to, crate::ui::theme::ACCENT, 2.0);
        }

        let is_connecting = self.dconn.is_some();
        for n in nodes {
            let vp = vpos[&n.id];
            let is_sel = self.sel == Some(n.id);
            let dpv = self
                .drag_param
                .as_ref()
                .filter(|dp| dp.node_id == n.id)
                .map(|dp| dp.current_value);
            self.paint_node(&painter, n, vp, is_sel, is_connecting, dpv, i18n);
        }

        if nodes.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                i18n.tr("node.right_click_hint"),
                FontId::proportional(14.0),
                crate::ui::theme::TEXT_SECONDARY,
            );
        }

        if mouse.is_some() {
            let hint = i18n.trf(
                "node.canvas_hint",
                &[
                    ("count", &nodes.len().to_string()),
                    ("zoom", &format!("{:.0}", self.zoom * 100.0)),
                ],
            );
            painter.text(
                pos2(rect.left() + 6.0, rect.bottom() - 16.0),
                Align2::LEFT_TOP,
                hint,
                FontId::proportional(9.0),
                crate::ui::theme::TEXT_SECONDARY,
            );
        }

        cmds
    }

    pub(super) fn zoom_to_fit(&mut self, nodes: &[NodeSnap], canvas_rect: Rect) {
        if nodes.is_empty() {
            return;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for n in nodes {
            let h = Self::node_h(n);
            min_x = min_x.min(n.pos.0);
            min_y = min_y.min(n.pos.1);
            max_x = max_x.max(n.pos.0 + NODE_W);
            max_y = max_y.max(n.pos.1 + h);
        }
        let margin = 60.0;
        let world_w = max_x - min_x + margin * 2.0;
        let world_h = max_y - min_y + margin * 2.0;
        if world_w <= 0.0 || world_h <= 0.0 {
            return;
        }
        let zoom_x = canvas_rect.width() / world_w;
        let zoom_y = canvas_rect.height() / world_h;
        self.zoom = zoom_x.min(zoom_y).clamp(0.2, 4.0);
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        self.pan = Vec2::new(
            canvas_rect.center().x - center_x * self.zoom,
            canvas_rect.center().y - center_y * self.zoom,
        );
    }
}
