use crate::ui::theme::*;
use egui::*;

pub fn icon_btn(ui: &mut Ui, icon: &str, tooltip: &str) -> Response {
    ui.add_sized([28.0, 24.0], Button::new(RichText::new(icon).size(15.0)))
        .on_hover_text(tooltip)
}

pub fn icon_btn_fill(ui: &mut Ui, icon: &str, tooltip: &str, fill: Color32) -> Response {
    ui.add_sized(
        [28.0, 24.0],
        Button::new(RichText::new(icon).size(15.0)).fill(fill),
    )
    .on_hover_text(tooltip)
}

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
    let radius = size * 0.38;

    let normalized = if (max - min).abs() < f32::EPSILON {
        0.0
    } else {
        (*value - min) / (max - min)
    };

    let fg_color = if *value > max * 0.85 {
        METER_RED
    } else if *value > max * 0.65 {
        METER_YELLOW
    } else {
        ACCENT
    };

    let start_angle = std::f32::consts::FRAC_PI_4 * 5.0;
    let end_angle = std::f32::consts::FRAC_PI_4;
    let sweep = start_angle - end_angle;
    let value_angle = start_angle - normalized * sweep;

    painter.circle_filled(
        center + vec2(0.0, 2.0),
        radius + 6.0,
        Color32::from_black_alpha(40),
    );

    painter.circle_filled(center, radius + 5.0, SURFACE_CONTAINER_HIGH);

    painter.circle_filled(center, radius + 3.0, SURFACE_CONTAINER);

    draw_arc_segment(
        &painter,
        center,
        radius,
        end_angle,
        start_angle,
        Stroke::new(5.0, KNOB_TRACK),
    );

    if normalized > 0.001 {
        draw_arc_segment(
            &painter,
            center,
            radius,
            value_angle,
            start_angle,
            Stroke::new(5.0, fg_color),
        );
    }

    let dot_r = 4.0;
    let ix = center.x + radius * value_angle.cos();
    let iy = center.y - radius * value_angle.sin();
    painter.circle_filled(
        Pos2::new(ix, iy) + vec2(0.0, 1.0),
        dot_r + 1.0,
        Color32::from_black_alpha(30),
    );
    painter.circle_filled(Pos2::new(ix, iy), dot_r, fg_color);

    painter.text(
        Pos2::new(center.x, rect.bottom() - 6.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(9.0),
        TEXT_SECONDARY,
    );

    let val_text = format!("{:.2}", value);
    painter.text(
        center + Vec2::new(0.0, radius * 0.3),
        Align2::CENTER_CENTER,
        val_text,
        FontId::proportional(8.0),
        TEXT_PRIMARY,
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
    let (rect, response) = ui.allocate_exact_size(Vec2::new(size * 2.2, size), Sense::click());

    if response.clicked() {
        return true;
    }

    let painter = ui.painter_at(rect);
    let center = rect.center();
    let track_height = size * 0.6;
    let track_r = CornerRadius::same((track_height / 2.0) as u8);
    let thumb_r = size * 0.32;

    let track_rect = Rect::from_center_size(center, Vec2::new(rect.width() - 6.0, track_height));

    painter.rect_filled(
        track_rect.translate(vec2(0.0, 1.0)),
        track_r,
        Color32::from_black_alpha(30),
    );

    if on {
        painter.rect_filled(track_rect, track_r, ACCENT_DIM);

        let glow_rect = track_rect.shrink(1.0);
        painter.rect_filled(
            glow_rect,
            track_r,
            Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), 40),
        );
    } else {
        painter.rect_filled(track_rect, track_r, OUTLINE_VAR);
    }

    let knob_x = if on {
        track_rect.right() - thumb_r - 4.0
    } else {
        track_rect.left() + thumb_r + 4.0
    };

    if on {
        painter.circle_filled(
            Pos2::new(knob_x, center.y) + vec2(0.0, 1.0),
            thumb_r + 1.0,
            Color32::from_black_alpha(30),
        );
        painter.circle_filled(Pos2::new(knob_x, center.y), thumb_r, ON_PRIMARY_CONTAINER);
    } else {
        painter.circle_filled(
            Pos2::new(knob_x, center.y) + vec2(0.0, 1.0),
            thumb_r + 1.0,
            Color32::from_black_alpha(30),
        );
        painter.circle_filled(
            Pos2::new(knob_x, center.y),
            thumb_r,
            SURFACE_CONTAINER_HIGHEST,
        );
        painter.circle_stroke(
            Pos2::new(knob_x, center.y),
            thumb_r,
            Stroke::new(1.5, OUTLINE),
        );
    }

    false
}
