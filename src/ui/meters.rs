use egui::*;

fn draw_meter_bars(ui: &mut Ui, label: &str, levels: &[f32], width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), crate::ui::theme::BG_PANEL);
    painter.rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0, crate::ui::theme::OUTLINE_VAR),
        StrokeKind::Inside,
    );

    painter.text(
        Pos2::new(rect.left() + 10.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(10.0),
        crate::ui::theme::TEXT_SECONDARY,
    );

    let meter_y = rect.top() + 24.0;
    let meter_h = (height - 30.0) / levels.len() as f32;
    let meter_w = width - 20.0;
    let meter_x = rect.left() + 10.0;

    for (i, level) in levels.iter().enumerate() {
        let y = meter_y + i as f32 * meter_h;

        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(meter_x, y),
                Pos2::new(meter_x + meter_w, y + meter_h - 2.0),
            ),
            CornerRadius::same(4),
            crate::ui::theme::SURFACE_CONTAINER_LOWEST,
        );

        let db = 20.0 * level.max(0.0001).log10();
        let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);

        let segments = 40;
        let gap = 1.5;
        let seg_w = (meter_w - gap * (segments as f32 - 1.0)) / segments as f32;
        let filled = (normalized * segments as f32) as usize;

        for s in 0..filled.min(segments) {
            let sx = meter_x + s as f32 * (seg_w + gap);
            let frac = s as f32 / segments as f32;
            let color = if frac > 0.85 {
                crate::ui::theme::METER_RED
            } else if frac > 0.65 {
                crate::ui::theme::METER_YELLOW
            } else {
                crate::ui::theme::METER_GREEN
            };
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(sx, y + 2.0),
                    Pos2::new(sx + seg_w, y + meter_h - 4.0),
                ),
                CornerRadius::same(2),
                color,
            );
        }
    }
}

pub fn draw_mono_meter(ui: &mut Ui, label: &str, level: f32, width: f32, height: f32) {
    draw_meter_bars(ui, label, &[level], width, height);
}

pub fn draw_stereo_meter(ui: &mut Ui, label: &str, left: f32, right: f32, width: f32, height: f32) {
    draw_meter_bars(ui, label, &[left, right], width, height);
}
