use egui::*;

pub fn draw_knob(ui: &mut Ui, value: &mut f32, label: &str, min: f32, max: f32, size: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click_and_drag());

    let changed = if response.dragged() {
        let delta = response.drag_delta().y * -0.005;
        *value = (*value + delta * (max - min)).clamp(min, max);
        true
    } else if response.double_clicked() {
        *value = (max - min) / 2.0 + min;
        true
    } else {
        false
    };

    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = size * 0.4;

    let normalized = if (max - min).abs() < f32::EPSILON {
        0.0
    } else {
        (*value - min) / (max - min)
    };

    let bg_color = crate::ui::theme::KNOB_TRACK;
    let fg_color = if *value > max * 0.85 {
        crate::ui::theme::METER_RED
    } else if *value > max * 0.65 {
        crate::ui::theme::METER_YELLOW
    } else {
        crate::ui::theme::ACCENT
    };

    let start_angle = std::f32::consts::FRAC_PI_4 * 5.0;
    let end_angle = std::f32::consts::FRAC_PI_4;
    let sweep = start_angle - end_angle;
    let value_angle = start_angle - normalized * sweep;

    draw_arc_segment(
        &painter,
        center,
        radius,
        end_angle,
        start_angle,
        Stroke::new(3.0, bg_color),
    );

    if normalized > 0.001 {
        draw_arc_segment(
            &painter,
            center,
            radius,
            value_angle,
            start_angle,
            Stroke::new(3.0, fg_color),
        );
    }

    let indicator_len = radius * 0.7;
    let ix = center.x + indicator_len * value_angle.cos();
    let iy = center.y - indicator_len * value_angle.sin();
    painter.line_segment(
        [center, Pos2::new(ix, iy)],
        Stroke::new(2.0, crate::ui::theme::TEXT_PRIMARY),
    );

    painter.text(
        Pos2::new(center.x, rect.bottom() - 8.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(9.0),
        crate::ui::theme::TEXT_SECONDARY,
    );

    let val_text = format!("{:.2}", value);
    painter.text(
        center + Vec2::new(0.0, radius * 0.3),
        Align2::CENTER_CENTER,
        val_text,
        FontId::proportional(8.0),
        crate::ui::theme::TEXT_PRIMARY,
    );

    changed
}

fn draw_arc_segment(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    from_angle: f32,
    to_angle: f32,
    stroke: Stroke,
) {
    let steps = 32;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let angle = from_angle + (to_angle - from_angle) * t;
            Pos2::new(
                center.x + radius * angle.cos(),
                center.y - radius * angle.sin(),
            )
        })
        .collect();
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}

pub fn draw_toggle(ui: &mut Ui, _label: &str, on: bool, size: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(size * 2.0, size), Sense::click());

    if response.clicked() {
        return true;
    }

    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = size * 0.4;

    painter.rect_filled(
        rect.shrink(2.0),
        CornerRadius::same(radius as u8),
        if on {
            crate::ui::theme::ACCENT_DIM
        } else {
            crate::ui::theme::KNOB_TRACK
        },
    );

    let knob_x = if on {
        rect.right() - radius - 4.0
    } else {
        rect.left() + radius + 4.0
    };

    painter.circle_filled(
        Pos2::new(knob_x, center.y),
        radius,
        crate::ui::theme::TEXT_PRIMARY,
    );

    false
}
