mod geometry;
mod hit_test;
mod interaction;
mod render;

use crate::audio::node::*;
use egui::*;

pub(super) const NODE_W: f32 = 180.0;
pub(super) const HDR_H: f32 = 24.0;
pub(super) const PORT_ROW: f32 = 20.0;
pub(super) const PORT_R: f32 = 5.0;
pub(super) const GRID_STEP: f32 = 30.0;
pub(super) const HIT_R: f32 = 10.0;
pub(super) const PARAM_ROW: f32 = 24.0;
pub(super) const PARAM_SENSITIVITY: f32 = 0.01;

pub(super) const COL_NODE_BG: Color32 = Color32::from_rgb(24, 24, 28);
pub(super) const COL_NODE_HDR: Color32 = Color32::from_rgb(58, 48, 70);
pub(super) const COL_PORT_MONO: Color32 = Color32::from_rgb(220, 172, 72);
pub(super) const COL_PORT_STEREO: Color32 = Color32::from_rgb(94, 220, 230);
pub(super) const COL_CONN: Color32 = Color32::from_rgb(178, 154, 214);
pub(super) const COL_CONN_HOVER: Color32 = Color32::from_rgb(240, 90, 90);
pub(super) const COL_GRID: Color32 = Color32::from_rgb(24, 24, 28);
pub(super) const COL_PARAM_BG: Color32 = Color32::from_rgb(20, 20, 28);

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

    pub(super) fn w2s(&self, p: Pos2) -> Pos2 {
        (p.to_vec2() * self.zoom + self.pan).to_pos2()
    }

    pub(super) fn s2w(&self, p: Pos2) -> Pos2 {
        ((p.to_vec2() - self.pan) / self.zoom).to_pos2()
    }
}
