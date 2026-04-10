use crate::audio::node::*;
use egui::*;

use super::{
    COL_GRID, COL_NODE_BG, COL_NODE_HDR, COL_PARAM_BG, GRID_STEP, HDR_H, PORT_R, NodeEditor,
    NodeSnap,
};

impl NodeEditor {
    pub(super) fn paint_grid(&self, p: &Painter, rect: Rect) {
        crate::ui::theme::paint_panel_texture(p, rect);
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

    pub(super) fn paint_bezier(
        &self,
        p: &Painter,
        from: Pos2,
        to: Pos2,
        color: Color32,
        width: f32,
    ) {
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
        let glow = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 34);
        p.line(pts.clone(), Stroke::new(width + 4.0, glow));
        p.line(pts, Stroke::new(width, color));
    }

    pub(super) fn paint_node(
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
            r.translate(vec2(0.0, 3.0)),
            10.0,
            Color32::from_black_alpha(70),
        );
        p.rect_filled(r, 10.0, bg);

        let hdr_r = Rect::from_min_max(r.min, pos2(r.right(), r.min.y + HDR_H * z));
        let hdr_col = if selected {
            crate::ui::theme::ACCENT_DIM
        } else {
            COL_NODE_HDR
        };
        p.rect_filled(hdr_r, 10.0, hdr_col);
        p.rect_filled(
            Rect::from_min_max(pos2(hdr_r.left(), hdr_r.bottom() - 3.0 * z), hdr_r.max),
            0.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
        );

        if selected {
            p.rect_stroke(
                r,
                10.0,
                Stroke::new(2.0, crate::ui::theme::ACCENT),
                egui::StrokeKind::Outside,
            );
        } else {
            p.rect_stroke(
                r,
                10.0,
                Stroke::new(1.0, crate::ui::theme::OUTLINE_VAR),
                egui::StrokeKind::Outside,
            );
        }

        for i in 0..3 {
            let y = r.top() + 6.0 * z + i as f32 * 4.0 * z;
            p.line_segment(
                [pos2(r.left() + 8.0 * z, y), pos2(r.left() + 14.0 * z, y)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 26)),
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
            p.circle_filled(
                pp,
                port_r + 4.0 * z,
                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 28),
            );
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
                p.circle_filled(
                    pp,
                    port_r + 4.0 * z,
                    Color32::from_rgba_unmultiplied(
                        crate::ui::theme::ACCENT.r(),
                        crate::ui::theme::ACCENT.g(),
                        crate::ui::theme::ACCENT.b(),
                        32,
                    ),
                );
                p.circle_filled(pp, port_r, crate::ui::theme::ACCENT);
            } else {
                p.circle_filled(
                    pp,
                    port_r + 4.0 * z,
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 28),
                );
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
                Stroke::new(1.0, crate::ui::theme::DIVIDER),
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
                    p.rect_filled(bar_rect, 5.0, Color32::from_rgb(14, 14, 18));
                    p.rect_filled(fill_r, 4.0, crate::ui::theme::ACCENT_DIM);
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
                    p.rect_filled(bar_rect, 5.0, Color32::from_rgb(14, 14, 18));
                    p.rect_filled(fill_r, 4.0, crate::ui::theme::ACCENT_DIM);
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
                    p.rect_filled(bar_rect, 5.0, Color32::from_rgb(14, 14, 18));
                    p.rect_filled(fill_r, 4.0, crate::ui::theme::ACCENT_DIM);
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
                    p.rect_filled(bar_rect, 5.0, Color32::from_rgb(14, 14, 18));
                    p.rect_filled(fill_r, 4.0, crate::ui::theme::ACCENT_DIM);
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
}
