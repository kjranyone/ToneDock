use crate::audio::node::*;
use egui::*;

use super::{
    COL_PORT_MONO, COL_PORT_STEREO, HDR_H, NODE_W, PARAM_ROW, PORT_ROW, NodeEditor, NodeSnap,
};

impl NodeEditor {
    pub(super) fn has_editable_param(n: &NodeSnap) -> bool {
        matches!(
            n.state,
            NodeInternalState::Gain { .. }
                | NodeInternalState::Pan { .. }
                | NodeInternalState::WetDry { .. }
                | NodeInternalState::SendBus { .. }
        )
    }

    pub(super) fn node_h(n: &NodeSnap) -> f32 {
        let extra = if Self::has_editable_param(n) {
            PARAM_ROW
        } else {
            0.0
        };
        HDR_H + n.inputs.len().max(n.outputs.len()).max(1) as f32 * PORT_ROW + extra
    }

    pub(super) fn node_srect(&self, pos: (f32, f32), h: f32) -> Rect {
        let min = self.w2s(Pos2::new(pos.0, pos.1));
        let max = min + vec2(NODE_W * self.zoom, h * self.zoom);
        Rect::from_min_max(min, max)
    }

    pub(super) fn param_srect(&self, pos: (f32, f32), n: &NodeSnap) -> Rect {
        let port_rows = n.inputs.len().max(n.outputs.len()).max(1) as f32 * PORT_ROW;
        let y_top = pos.1 + HDR_H + port_rows;
        let min = self.w2s(Pos2::new(pos.0, y_top));
        let max = min + vec2(NODE_W * self.zoom, PARAM_ROW * self.zoom);
        Rect::from_min_max(min, max)
    }

    pub(super) fn in_spos(&self, pos: (f32, f32), idx: usize) -> Pos2 {
        let y = pos.1 + HDR_H + idx as f32 * PORT_ROW + PORT_ROW * 0.5;
        self.w2s(Pos2::new(pos.0, y))
    }

    pub(super) fn out_spos(&self, pos: (f32, f32), idx: usize) -> Pos2 {
        let y = pos.1 + HDR_H + idx as f32 * PORT_ROW + PORT_ROW * 0.5;
        self.w2s(Pos2::new(pos.0 + NODE_W, y))
    }

    pub(super) fn node_label(t: &NodeType) -> &str {
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

    pub(super) fn ch_label(c: ChannelConfig) -> &'static str {
        match c {
            ChannelConfig::Mono => "M",
            ChannelConfig::Stereo => "S",
            ChannelConfig::Custom(_) => "?",
        }
    }

    pub(super) fn port_color(c: ChannelConfig) -> Color32 {
        match c {
            ChannelConfig::Mono => COL_PORT_MONO,
            ChannelConfig::Stereo => COL_PORT_STEREO,
            ChannelConfig::Custom(_) => Color32::GRAY,
        }
    }
}
