use std::collections::HashMap;

use crate::audio::node::*;
use egui::*;

const NODE_W: f32 = 160.0;
const HDR_H: f32 = 24.0;
const PORT_ROW: f32 = 20.0;
const PORT_R: f32 = 5.0;
const GRID_STEP: f32 = 30.0;
const HIT_R: f32 = 10.0;
const PARAM_ROW: f32 = 24.0;
const PARAM_SENSITIVITY: f32 = 0.01;

const COL_NODE_BG: Color32 = Color32::from_rgb(50, 50, 60);
const COL_NODE_HDR: Color32 = Color32::from_rgb(40, 40, 50);
const COL_PORT_MONO: Color32 = Color32::from_rgb(220, 180, 50);
const COL_PORT_STEREO: Color32 = Color32::from_rgb(50, 180, 220);
const COL_CONN: Color32 = Color32::from_rgb(140, 140, 160);
const COL_CONN_HOVER: Color32 = Color32::from_rgb(220, 80, 80);
const COL_GRID: Color32 = Color32::from_rgb(38, 38, 45);
const COL_PARAM_BG: Color32 = Color32::from_rgb(35, 35, 45);
const COL_PARAM_FILL: Color32 = Color32::from_rgb(70, 100, 160);

pub struct NodeSnap {
    pub id: NodeId,
    pub node_type: NodeType,
    pub enabled: bool,
    pub bypassed: bool,
    pub pos: (f32, f32),
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub state: NodeInternalState,
}

pub enum EdCmd {
    AddNode(NodeType, (f32, f32)),
    AddVstNode {
        plugin_path: std::path::PathBuf,
        plugin_name: String,
        pos: (f32, f32),
    },
    RemoveNode(NodeId),
    Connect(Connection),
    Disconnect(NodeId, PortId, NodeId, PortId),
    SetPos(NodeId, f32, f32),
    SetState(NodeId, NodeInternalState),
    ToggleBypass(NodeId),
    DuplicateNode(NodeId),
    #[allow(dead_code)]
    SetVstParameter {
        node_id: NodeId,
        param_index: usize,
        value: f32,
    },
    Commit,
    ApplyTemplate(String, (f32, f32)),
}

struct DConn {
    src_node: NodeId,
    src_port: PortId,
    from: Pos2,
    to: Pos2,
}

struct DragParam {
    node_id: NodeId,
    start_x: f32,
    start_value: f32,
    current_value: f32,
}

pub struct NodeEditor {
    pan: Vec2,
    zoom: f32,
    sel: Option<NodeId>,
    drag_node: Option<NodeId>,
    drag_off: Vec2,
    dconn: Option<DConn>,
    drag_param: Option<DragParam>,
    ptr_down: bool,
    menu_wpos: Pos2,
    hover_conn: Option<usize>,
    menu_conn: Option<usize>,
}

impl NodeEditor {
    pub fn new() -> Self {
        Self {
            pan: Vec2::new(100.0, 100.0),
            zoom: 1.0,
            sel: None,
            drag_node: None,
            drag_off: Vec2::ZERO,
            dconn: None,
            drag_param: None,
            ptr_down: false,
            menu_wpos: Pos2::ZERO,
            hover_conn: None,
            menu_conn: None,
        }
    }

    pub fn set_selection(&mut self, id: Option<NodeId>) {
        self.sel = id;
    }

    pub fn selected_node(&self) -> Option<NodeId> {
        self.sel
    }

    fn w2s(&self, p: Pos2) -> Pos2 {
        (p.to_vec2() * self.zoom + self.pan).to_pos2()
    }

    fn s2w(&self, p: Pos2) -> Pos2 {
        ((p.to_vec2() - self.pan) / self.zoom).to_pos2()
    }

    fn has_editable_param(n: &NodeSnap) -> bool {
        matches!(
            n.state,
            NodeInternalState::Gain { .. }
                | NodeInternalState::Pan { .. }
                | NodeInternalState::WetDry { .. }
                | NodeInternalState::SendBus { .. }
        )
    }

    fn node_h(n: &NodeSnap) -> f32 {
        let extra = if Self::has_editable_param(n) {
            PARAM_ROW
        } else {
            0.0
        };
        HDR_H + n.inputs.len().max(n.outputs.len()).max(1) as f32 * PORT_ROW + extra
    }

    fn node_srect(&self, pos: (f32, f32), h: f32) -> Rect {
        let min = self.w2s(Pos2::new(pos.0, pos.1));
        let max = min + vec2(NODE_W * self.zoom, h * self.zoom);
        Rect::from_min_max(min, max)
    }

    fn param_srect(&self, pos: (f32, f32), n: &NodeSnap) -> Rect {
        let port_rows = n.inputs.len().max(n.outputs.len()).max(1) as f32 * PORT_ROW;
        let y_top = pos.1 + HDR_H + port_rows;
        let min = self.w2s(Pos2::new(pos.0, y_top));
        let max = min + vec2(NODE_W * self.zoom, PARAM_ROW * self.zoom);
        Rect::from_min_max(min, max)
    }

    fn in_spos(&self, pos: (f32, f32), idx: usize) -> Pos2 {
        let y = pos.1 + HDR_H + idx as f32 * PORT_ROW + PORT_ROW * 0.5;
        self.w2s(Pos2::new(pos.0, y))
    }

    fn out_spos(&self, pos: (f32, f32), idx: usize) -> Pos2 {
        let y = pos.1 + HDR_H + idx as f32 * PORT_ROW + PORT_ROW * 0.5;
        self.w2s(Pos2::new(pos.0 + NODE_W, y))
    }

    fn node_label(t: &NodeType) -> &str {
        match t {
            NodeType::AudioInput => "Audio In",
            NodeType::AudioOutput => "Audio Out",
            NodeType::VstPlugin { plugin_name, .. } => plugin_name.as_str(),
            NodeType::Gain => "Gain",
            NodeType::Pan => "Pan",
            NodeType::Mixer { .. } => "Mixer",
            NodeType::Splitter { .. } => "Splitter",
            NodeType::ChannelConverter { .. } => "Converter",
            NodeType::Metronome => "Metronome",
            NodeType::Looper => "Looper",
            NodeType::WetDry => "Wet/Dry",
            NodeType::SendBus { .. } => "Send",
            NodeType::ReturnBus { .. } => "Return",
        }
    }

    fn ch_label(c: ChannelConfig) -> &'static str {
        match c {
            ChannelConfig::Mono => "M",
            ChannelConfig::Stereo => "S",
            ChannelConfig::Custom(_) => "?",
        }
    }

    fn port_color(c: ChannelConfig) -> Color32 {
        match c {
            ChannelConfig::Mono => COL_PORT_MONO,
            ChannelConfig::Stereo => COL_PORT_STEREO,
            ChannelConfig::Custom(_) => Color32::GRAY,
        }
    }

    fn hit_node(
        &self,
        nodes: &[NodeSnap],
        mp: Pos2,
        vpos: &HashMap<NodeId, (f32, f32)>,
    ) -> Option<NodeId> {
        let mut hit: Option<(NodeId, f32)> = None;
        for n in nodes {
            let pos = vpos[&n.id];
            let h = Self::node_h(n);
            let r = self.node_srect(pos, h);
            if r.contains(mp) {
                let center_y = r.center().y;
                match hit {
                    None => hit = Some((n.id, center_y)),
                    Some((_, prev_y)) => {
                        if center_y > prev_y {
                            hit = Some((n.id, center_y));
                        }
                    }
                }
            }
        }
        hit.map(|(id, _)| id)
    }

    fn hit_out_port(
        &self,
        nodes: &[NodeSnap],
        mp: Pos2,
        vpos: &HashMap<NodeId, (f32, f32)>,
    ) -> Option<(NodeId, PortId, usize)> {
        let effective_r = HIT_R.max(PORT_R * self.zoom);
        for n in nodes {
            let pos = vpos[&n.id];
            for (i, port) in n.outputs.iter().enumerate() {
                let pp = self.out_spos(pos, i);
                if (pp - mp).length() < effective_r {
                    return Some((n.id, port.id, i));
                }
            }
        }
        None
    }

    fn hit_in_port(
        &self,
        nodes: &[NodeSnap],
        mp: Pos2,
        vpos: &HashMap<NodeId, (f32, f32)>,
    ) -> Option<(NodeId, PortId)> {
        let effective_r = HIT_R.max(PORT_R * self.zoom);
        for n in nodes {
            let pos = vpos[&n.id];
            for (i, port) in n.inputs.iter().enumerate() {
                let pp = self.in_spos(pos, i);
                if (pp - mp).length() < effective_r {
                    return Some((n.id, port.id));
                }
            }
        }
        None
    }

    fn hit_param(
        &self,
        nodes: &[NodeSnap],
        mp: Pos2,
        vpos: &HashMap<NodeId, (f32, f32)>,
    ) -> Option<NodeId> {
        for n in nodes {
            if !Self::has_editable_param(n) {
                continue;
            }
            let r = self.param_srect(vpos[&n.id], n);
            if r.contains(mp) {
                return Some(n.id);
            }
        }
        None
    }

    fn hit_connection(
        &self,
        conns: &[Connection],
        nodes: &[NodeSnap],
        vpos: &HashMap<NodeId, (f32, f32)>,
        mp: Pos2,
    ) -> Option<usize> {
        let threshold = 8.0 * self.zoom.max(0.5);
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
                    if Self::point_near_bezier(from, to, mp, threshold) {
                        return Some(ci);
                    }
                }
            }
        }
        None
    }

    fn point_near_bezier(from: Pos2, to: Pos2, point: Pos2, threshold: f32) -> bool {
        let dx = (to.x - from.x).abs().max(50.0);
        let p1 = Pos2::new(from.x + dx * 0.5, from.y);
        let p2 = Pos2::new(to.x - dx * 0.5, to.y);
        let steps = 20;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let mt = 1.0 - t;
            let x = mt * mt * mt * from.x
                + 3.0 * mt * mt * t * p1.x
                + 3.0 * mt * t * t * p2.x
                + t * t * t * to.x;
            let y = mt * mt * mt * from.y
                + 3.0 * mt * mt * t * p1.y
                + 3.0 * mt * t * t * p2.y
                + t * t * t * to.y;
            if (Pos2::new(x, y) - point).length() < threshold {
                return true;
            }
        }
        false
    }

    fn zoom_to_fit(&mut self, nodes: &[NodeSnap], canvas_rect: Rect) {
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

    fn paint_grid(&self, p: &Painter, rect: Rect) {
        let step = GRID_STEP * self.zoom;
        if step < 5.0 {
            return;
        }
        let ox = self.pan.x % step;
        let oy = self.pan.y % step;
        let mut x = rect.left() + ox;
        while x < rect.right() {
            p.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, COL_GRID),
            );
            x += step;
        }
        let mut y = rect.top() + oy;
        while y < rect.bottom() {
            p.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, COL_GRID),
            );
            y += step;
        }
    }

    fn paint_bezier(&self, p: &Painter, from: Pos2, to: Pos2, color: Color32, width: f32) {
        let dx = (to.x - from.x).abs().max(50.0);
        let p1 = Pos2::new(from.x + dx * 0.5, from.y);
        let p2 = Pos2::new(to.x - dx * 0.5, to.y);
        let steps = 20;
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let mt = 1.0 - t;
            let x = mt * mt * mt * from.x
                + 3.0 * mt * mt * t * p1.x
                + 3.0 * mt * t * t * p2.x
                + t * t * t * to.x;
            let y = mt * mt * mt * from.y
                + 3.0 * mt * mt * t * p1.y
                + 3.0 * mt * t * t * p2.y
                + t * t * t * to.y;
            pts.push(Pos2::new(x, y));
        }
        p.line(pts, Stroke::new(width, color));
    }

    fn paint_node(
        &self,
        p: &Painter,
        n: &NodeSnap,
        vpos: (f32, f32),
        selected: bool,
        is_connect_target: bool,
        drag_param_value: Option<f32>,
    ) {
        let h = Self::node_h(n);
        let r = self.node_srect(vpos, h);
        let z = self.zoom;

        let bg = if !n.enabled {
            crate::ui::theme::DISABLED
        } else if n.bypassed {
            crate::ui::theme::BYPASSED
        } else {
            COL_NODE_BG
        };

        p.rect_filled(
            r.translate(vec2(2.0, 2.0)),
            4.0,
            Color32::from_black_alpha(50),
        );
        p.rect_filled(r, 4.0, bg);

        let hdr_r = Rect::from_min_max(r.min, pos2(r.right(), r.min.y + HDR_H * z));
        let hdr_col = if selected {
            crate::ui::theme::ACCENT_DIM
        } else {
            COL_NODE_HDR
        };
        p.rect_filled(hdr_r, 4.0, hdr_col);

        if selected {
            p.rect_stroke(
                r,
                4.0,
                Stroke::new(2.0, crate::ui::theme::ACCENT),
                egui::StrokeKind::Outside,
            );
        } else {
            p.rect_stroke(
                r,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(70, 70, 80)),
                egui::StrokeKind::Outside,
            );
        }

        let fs = (11.0 * z).clamp(7.0, 16.0);
        let port_fs = (9.0 * z).clamp(6.0, 14.0);
        let port_r = PORT_R * z;

        p.text(
            pos2(r.min.x + 6.0, r.min.y + (HDR_H * z - fs) * 0.5),
            Align2::LEFT_TOP,
            Self::node_label(&n.node_type),
            FontId::proportional(fs),
            crate::ui::theme::TEXT_PRIMARY,
        );

        for (i, port) in n.inputs.iter().enumerate() {
            let pp = self.in_spos(vpos, i);
            let col = Self::port_color(port.channels);
            p.circle_filled(pp, port_r, col);
            p.circle(
                pp,
                port_r,
                Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::WHITE),
            );
            p.text(
                pos2(pp.x + port_r + 3.0, pp.y - port_fs * 0.5),
                Align2::LEFT_TOP,
                format!("{} ({})", port.name, Self::ch_label(port.channels)),
                FontId::proportional(port_fs),
                crate::ui::theme::TEXT_SECONDARY,
            );
        }

        for (i, port) in n.outputs.iter().enumerate() {
            let pp = self.out_spos(vpos, i);
            let col = Self::port_color(port.channels);
            if is_connect_target {
                p.circle_filled(pp, port_r, crate::ui::theme::ACCENT);
            } else {
                p.circle_filled(pp, port_r, col);
            }
            p.circle(
                pp,
                port_r,
                Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::WHITE),
            );
            p.text(
                pos2(pp.x - port_r - 3.0, pp.y - port_fs * 0.5),
                Align2::RIGHT_TOP,
                format!("{} ({})", port.name, Self::ch_label(port.channels)),
                FontId::proportional(port_fs),
                crate::ui::theme::TEXT_SECONDARY,
            );
        }

        if Self::has_editable_param(n) {
            let param_r = self.param_srect(vpos, n);

            p.line_segment(
                [
                    pos2(param_r.min.x, param_r.min.y),
                    pos2(param_r.max.x, param_r.min.y),
                ],
                Stroke::new(1.0, Color32::from_rgb(60, 60, 70)),
            );
            p.rect_filled(param_r, 0.0, COL_PARAM_BG);

            let value = drag_param_value.unwrap_or_else(|| match &n.state {
                NodeInternalState::Gain { value } => *value,
                NodeInternalState::Pan { value } => *value,
                NodeInternalState::WetDry { mix } => *mix,
                NodeInternalState::SendBus { send_level } => *send_level,
                _ => 0.0,
            });

            let param_fs = (10.0 * z).clamp(7.0, 14.0);
            let padding = 4.0 * z;
            let bar_rect = Rect::from_min_max(
                pos2(param_r.min.x + padding, param_r.center().y - 4.0 * z),
                pos2(param_r.max.x - padding, param_r.center().y + 4.0 * z),
            );

            match &n.state {
                NodeInternalState::Gain { .. } => {
                    let fill_ratio = (value / 4.0).clamp(0.0, 1.0);
                    let fill_w = bar_rect.width() * fill_ratio;
                    let fill_r = Rect::from_min_max(
                        bar_rect.min,
                        pos2(bar_rect.min.x + fill_w, bar_rect.max.y),
                    );
                    p.rect_filled(bar_rect, 2.0, Color32::from_rgb(25, 25, 35));
                    p.rect_filled(fill_r, 2.0, COL_PARAM_FILL);
                    p.text(
                        pos2(param_r.min.x + padding, bar_rect.min.y - param_fs - 1.0 * z),
                        Align2::LEFT_BOTTOM,
                        format!("{:.2}", value),
                        FontId::proportional(param_fs),
                        crate::ui::theme::TEXT_PRIMARY,
                    );
                }
                NodeInternalState::Pan { .. } => {
                    let fill_ratio = ((value + 1.0) / 2.0).clamp(0.0, 1.0);
                    let fill_w = bar_rect.width() * fill_ratio;
                    let fill_r = Rect::from_min_max(
                        bar_rect.min,
                        pos2(bar_rect.min.x + fill_w, bar_rect.max.y),
                    );
                    p.rect_filled(bar_rect, 2.0, Color32::from_rgb(25, 25, 35));
                    p.rect_filled(fill_r, 2.0, COL_PARAM_FILL);
                    let label = if value < -0.01 {
                        format!("L {:.2}", value.abs())
                    } else if value > 0.01 {
                        format!("R {:.2}", value.abs())
                    } else {
                        "C".into()
                    };
                    p.text(
                        pos2(param_r.min.x + padding, bar_rect.min.y - param_fs - 1.0 * z),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional(param_fs),
                        crate::ui::theme::TEXT_PRIMARY,
                    );
                }
                NodeInternalState::WetDry { .. } => {
                    let fill_ratio = value.clamp(0.0, 1.0);
                    let fill_w = bar_rect.width() * fill_ratio;
                    let fill_r = Rect::from_min_max(
                        bar_rect.min,
                        pos2(bar_rect.min.x + fill_w, bar_rect.max.y),
                    );
                    p.rect_filled(bar_rect, 2.0, Color32::from_rgb(25, 25, 35));
                    p.rect_filled(fill_r, 2.0, COL_PARAM_FILL);
                    let label = format!("Wet {:.0}%", value * 100.0);
                    p.text(
                        pos2(param_r.min.x + padding, bar_rect.min.y - param_fs - 1.0 * z),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional(param_fs),
                        crate::ui::theme::TEXT_PRIMARY,
                    );
                }
                NodeInternalState::SendBus { .. } => {
                    let fill_ratio = value.clamp(0.0, 1.0);
                    let fill_w = bar_rect.width() * fill_ratio;
                    let fill_r = Rect::from_min_max(
                        bar_rect.min,
                        pos2(bar_rect.min.x + fill_w, bar_rect.max.y),
                    );
                    p.rect_filled(bar_rect, 2.0, Color32::from_rgb(25, 25, 35));
                    p.rect_filled(fill_r, 2.0, COL_PARAM_FILL);
                    let label = format!("Send {:.0}%", value * 100.0);
                    p.text(
                        pos2(param_r.min.x + padding, bar_rect.min.y - param_fs - 1.0 * z),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::proportional(param_fs),
                        crate::ui::theme::TEXT_PRIMARY,
                    );
                }
                _ => {}
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        nodes: &[NodeSnap],
        conns: &[Connection],
        available_plugins: &[crate::vst_host::scanner::PluginInfo],
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
                        RichText::new("Connection")
                            .strong()
                            .color(crate::ui::theme::ACCENT),
                    );
                    ui.separator();
                    if ui.button("Delete Connection").clicked() {
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
                RichText::new("Add Node")
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button("Gain").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Gain, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Pan").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Pan, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Splitter (2-out)").clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::Splitter { outputs: 2 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Mixer (2-in)").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Mixer { inputs: 2 }, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Converter (M\u{2192}S)").clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Stereo,
                    },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Converter (S\u{2192}M)").clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ChannelConverter {
                        target: ChannelConfig::Mono,
                    },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Metronome").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Metronome, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Looper").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::Looper, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Wet/Dry").clicked() {
                menu_cmds.push(EdCmd::AddNode(NodeType::WetDry, (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            ui.separator();
            ui.label(
                RichText::new("Send/Return Buses")
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button("Send Bus #1").clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::SendBus { bus_id: 1 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Return Bus #1").clicked() {
                menu_cmds.push(EdCmd::AddNode(
                    NodeType::ReturnBus { bus_id: 1 },
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            ui.separator();
            ui.label(
                RichText::new("Templates")
                    .strong()
                    .color(crate::ui::theme::ACCENT),
            );
            ui.separator();
            if ui.button("Wide Stereo Amp").clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("wide_stereo_amp".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Dry/Wet Blend").clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("dry_wet_blend".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Send/Return Reverb").clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate(
                    "send_return_reverb".into(),
                    (mw.x, mw.y),
                ));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }
            if ui.button("Parallel Chain").clicked() {
                menu_cmds.push(EdCmd::ApplyTemplate("parallel_chain".into(), (mw.x, mw.y)));
                menu_cmds.push(EdCmd::Commit);
                ui.close_menu();
            }

            if !available_plugins.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new("VST Plugins")
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
                ui.label(RichText::new("Selected Node").color(crate::ui::theme::TEXT_SECONDARY));
                if let Some(bp) = sel_bypassed {
                    let text = if bp { "Unbypass" } else { "Bypass" };
                    if ui.button(text).clicked() {
                        menu_cmds.push(EdCmd::ToggleBypass(sid));
                        ui.close_menu();
                    }
                }
                if sel_is_io != Some(true) {
                    if ui.button("Delete").clicked() {
                        menu_cmds.push(EdCmd::RemoveNode(sid));
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                    if ui.button("Duplicate").clicked() {
                        menu_cmds.push(EdCmd::DuplicateNode(sid));
                        menu_cmds.push(EdCmd::Commit);
                        ui.close_menu();
                    }
                }
            }

            ui.separator();
            if ui.button("Reset View").clicked() {
                self.pan = Vec2::new(100.0, 100.0);
                self.zoom = 1.0;
                ui.close_menu();
            }
            if ui.button("Fit All (F)").clicked() {
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

        // --- Painting ---
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
            self.paint_node(&painter, n, vp, is_sel, is_connecting, dpv);
        }

        if nodes.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Right-click to add nodes",
                FontId::proportional(14.0),
                crate::ui::theme::TEXT_SECONDARY,
            );
        }

        if mouse.is_some() {
            let hint = format!(
                "Nodes: {}  Zoom: {:.0}%  LMB: select/drag  RMB: menu  Scroll: zoom  F: fit  Ctrl+D: duplicate",
                nodes.len(),
                self.zoom * 100.0
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
}
