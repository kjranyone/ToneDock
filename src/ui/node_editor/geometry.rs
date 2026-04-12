use std::borrow::Cow;

use crate::audio::node::*;
use crate::i18n::I18n;
use egui::*;

use super::{
    NodeEditor, NodeSnap, COL_PORT_MONO, COL_PORT_STEREO, HDR_H, NODE_W, PARAM_ROW, PORT_ROW,
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

    pub(super) fn node_label<'a>(t: &'a NodeType, i18n: &'a I18n) -> Cow<'a, str> {
        match t {
            NodeType::AudioInput => Cow::Borrowed(i18n.tr("node.audio_in")),
            NodeType::AudioOutput => Cow::Borrowed(i18n.tr("node.audio_out")),
            NodeType::VstPlugin { plugin_name, .. } => Cow::Borrowed(plugin_name.as_str()),
            NodeType::Gain => Cow::Borrowed(i18n.tr("node.gain")),
            NodeType::Pan => Cow::Borrowed(i18n.tr("node.pan")),
            NodeType::Mixer { .. } => Cow::Borrowed(i18n.tr("node.mixer")),
            NodeType::Splitter { .. } => Cow::Borrowed(i18n.tr("node.splitter")),
            NodeType::ChannelConverter { .. } => Cow::Borrowed(i18n.tr("node.converter")),
            NodeType::Metronome => Cow::Borrowed(i18n.tr("node.metronome")),
            NodeType::Looper => Cow::Borrowed(i18n.tr("node.looper")),
            NodeType::WetDry => Cow::Borrowed(i18n.tr("node.wet_dry")),
            NodeType::SendBus { .. } => Cow::Borrowed(i18n.tr("node.send")),
            NodeType::ReturnBus { .. } => Cow::Borrowed(i18n.tr("node.return")),
        }
    }

    pub(super) fn ch_label<'a>(c: ChannelConfig, i18n: &'a I18n) -> &'a str {
        match c {
            ChannelConfig::Mono => i18n.tr("node.ch_mono"),
            ChannelConfig::Stereo => i18n.tr("node.ch_stereo"),
            ChannelConfig::Custom(_) => i18n.tr("node.ch_custom"),
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
