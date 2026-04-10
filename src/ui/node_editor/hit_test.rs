use std::collections::HashMap;

use crate::audio::node::*;
use egui::*;

use super::{HIT_R, PORT_R, NodeEditor, NodeSnap};

impl NodeEditor {
    pub(super) fn hit_node(
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

    pub(super) fn hit_out_port(
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

    pub(super) fn hit_in_port(
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

    pub(super) fn hit_param(
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

    pub(super) fn hit_connection(
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

    pub(super) fn point_near_bezier(from: Pos2, to: Pos2, point: Pos2, threshold: f32) -> bool {
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
}
