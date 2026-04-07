use egui::*;

pub fn draw_stereo_meter(ui: &mut Ui, label: &str, left: f32, right: f32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    let painter = ui.painter_at(rect);

    painter.text(
        Pos2::new(rect.left(), rect.top()),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(10.0),
        crate::ui::theme::TEXT_SECONDARY,
    );

    let meter_y = rect.top() + 14.0;
    let meter_h = (height - 20.0) / 2.0;
    let meter_w = width - 20.0;
    let meter_x = rect.left() + 10.0;

    for (i, level) in [left, right].iter().enumerate() {
        let y = meter_y + i as f32 * (meter_h + 2.0);

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
