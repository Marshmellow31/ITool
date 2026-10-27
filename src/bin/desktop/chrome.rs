use super::*;

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Photo,
    Video,
    Live,
    Calendar,
    Folder,
    Search,
    Grid,
    Sort,
    Options,
    Sun,
    Moon,
    Settings,
    Arrow,
    Check,
    File,
    Monitor,
    Minimize,
    Maximize,
    Close,
}

pub(crate) fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).font(egui::FontId::new(
        size,
        egui::FontFamily::Name("UI Semibold".into()),
    ))
}

pub(crate) fn draw_icon(p: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let center = rect.center();
    let s = rect.width().min(rect.height()) / 20.0;
    let pt = |x: f32, y: f32| center + Vec2::new(x * s, y * s);
    let stroke = Stroke::new(1.5 * s, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], stroke);
    };
    let box_at = |a: (f32, f32), b: (f32, f32)| {
        p.rect_stroke(
            egui::Rect::from_min_max(pt(a.0, a.1), pt(b.0, b.1)),
            2,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Photo => {
            box_at((-8., -7.), (8., 7.));
            p.circle_stroke(pt(3., -2.), 1.5 * s, stroke);
            line((-6., 5.), (-2., 0.));
            line((-2., 0.), (2., 4.));
            line((2., 4.), (4., 2.));
            line((4., 2.), (7., 5.));
        }
        Icon::Video => {
            box_at((-8., -6.), (5., 6.));
            line((5., -3.), (9., -5.));
            line((9., -5.), (9., 5.));
            line((9., 5.), (5., 3.));
        }
        Icon::Live => {
            for r in [3., 6., 9.] {
                p.circle_stroke(center, r * s, stroke);
            }
        }
        Icon::Calendar => {
            box_at((-8., -7.), (8., 8.));
            line((-8., -2.), (8., -2.));
            line((-4., -9.), (-4., -5.));
            line((4., -9.), (4., -5.));
        }
        Icon::Folder => {
            box_at((-9., -4.), (9., 7.));
            line((-9., -4.), (-9., -7.));
            line((-9., -7.), (-2., -7.));
            line((-2., -7.), (1., -4.));
        }
        Icon::Search => {
            p.circle_stroke(pt(-2., -2.), 6. * s, stroke);
            line((3., 3.), (8., 8.));
        }
        Icon::Grid => {
            for x in [-7., 2.] {
                for y in [-7., 2.] {
                    box_at((x, y), (x + 5., y + 5.));
                }
            }
        }
        Icon::Sort => {
            line((-4., -7.), (-4., 7.));
            line((-7., -4.), (-4., -7.));
            line((-4., -7.), (-1., -4.));
            line((4., -7.), (4., 7.));
            line((1., 4.), (4., 7.));
            line((4., 7.), (7., 4.));
        }
        Icon::Options => {
            for y in [-6., 0., 6.] {
                line((-8., y), (8., y));
            }
            for (x, y) in [(-3., -6.), (4., 0.), (-1., 6.)] {
                p.circle_filled(pt(x, y), 2. * s, color);
            }
        }
        Icon::Sun => {
            p.circle_stroke(center, 4. * s, stroke);
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::TAU / 8.;
                p.line_segment(
                    [
                        center + Vec2::angled(a) * 7. * s,
                        center + Vec2::angled(a) * 9. * s,
                    ],
                    stroke,
                );
            }
        }
        Icon::Moon => {
            p.add(egui::epaint::PathShape::line(
                vec![
                    pt(0., -8.),
                    pt(-5., -5.),
                    pt(-7., 0.),
                    pt(-5., 6.),
                    pt(1., 8.),
                    pt(7., 5.),
                    pt(2., 5.),
                    pt(-2., 1.),
                    pt(-2., -4.),
                    pt(0., -8.),
                ],
                stroke,
            ));
        }
        Icon::Settings => {
            p.circle_stroke(center, 5.5 * s, stroke);
            p.circle_stroke(center, 2. * s, stroke);
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::TAU / 8.;
                p.line_segment(
                    [
                        center + Vec2::angled(a) * 6. * s,
                        center + Vec2::angled(a) * 8.5 * s,
                    ],
                    stroke,
                );
            }
        }
        Icon::Arrow => {
            line((-7., 0.), (7., 0.));
            line((2., -5.), (7., 0.));
            line((7., 0.), (2., 5.));
        }
        Icon::Check => {
            line((-6., 0.), (-1., 5.));
            line((-1., 5.), (7., -5.));
        }
        Icon::File => {
            p.add(egui::epaint::PathShape::closed_line(
                vec![
                    pt(-6., -9.),
                    pt(2., -9.),
                    pt(7., -4.),
                    pt(7., 9.),
                    pt(-6., 9.),
                ],
                stroke,
            ));
            line((2., -9.), (2., -4.));
            line((2., -4.), (7., -4.));
        }
        Icon::Monitor => {
            box_at((-9., -7.), (9., 5.));
            line((0., 5.), (0., 9.));
            line((-5., 9.), (5., 9.));
        }
        Icon::Minimize => line((-6., 0.), (6., 0.)),
        Icon::Maximize => box_at((-6., -6.), (6., 6.)),
        Icon::Close => {
            line((-6., -6.), (6., 6.));
            line((-6., 6.), (6., -6.));
        }
    }
}

pub(crate) fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let p = Palette::new(ui.visuals().dark_mode);
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(34.0)),
    );
    draw_icon(
        ui.painter(),
        response.rect.shrink(8.0),
        icon,
        if response.hovered() { p.accent } else { p.text },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_text(label)
}

pub(crate) fn nav(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let p = Palette::new(ui.visuals().dark_mode);
    let response = ui.add_sized(
        [ui.available_width(), 40.0],
        egui::Button::new("")
            .fill(if selected {
                p.tint
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::NONE)
            .corner_radius(6),
    );
    let color = if selected { p.accent } else { p.text };
    let rect = egui::Rect::from_center_size(
        response.rect.left_center() + Vec2::new(22., 0.),
        Vec2::splat(18.),
    );
    draw_icon(ui.painter(), rect, icon, color);
    ui.painter().text(
        response.rect.left_center() + Vec2::new(46., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(
            14.,
            if selected {
                egui::FontFamily::Name("UI Semibold".into())
            } else {
                egui::FontFamily::Proportional
            },
        ),
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    response
}

pub(crate) fn step(
    ui: &mut egui::Ui,
    number: usize,
    label: &str,
    active: bool,
    enabled: bool,
) -> egui::Response {
    let p = Palette::new(ui.visuals().dark_mode);
    let width = if label == "Choose media" { 136. } else { 104. };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::new(width, 38.)),
    );
    let center = response.rect.left_center() + Vec2::new(15., 0.);
    ui.painter()
        .circle_filled(center, 13., if active { ACCENT } else { p.canvas });
    if !active {
        ui.painter()
            .circle_stroke(center, 13., Stroke::new(1., p.border));
    }
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        number.to_string(),
        egui::FontId::new(13., egui::FontFamily::Name("UI Semibold".into())),
        if active { Color32::WHITE } else { p.muted },
    );
    ui.painter().text(
        center + Vec2::new(24., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(
            12.,
            if active {
                egui::FontFamily::Name("UI Semibold".into())
            } else {
                egui::FontFamily::Proportional
            },
        ),
        if active { p.accent } else { p.muted },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    response
}

pub(crate) fn menu(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let p = Palette::new(ui.visuals().dark_mode);
    let (response, _) =
        egui::menu::MenuButton::from_button(egui::Button::new("").min_size(Vec2::new(width, 38.)))
            .ui(ui, content);
    draw_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            response.rect.left_center() + Vec2::new(18., 0.),
            Vec2::splat(16.),
        ),
        icon,
        p.text,
    );
    ui.painter().text(
        response.rect.left_center() + Vec2::new(34., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.),
        p.text,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let c = response.rect.right_center() - Vec2::new(12., 0.);
    ui.painter().add(egui::epaint::PathShape::line(
        vec![
            c + Vec2::new(-3., -1.),
            c + Vec2::new(0., 2.),
            c + Vec2::new(3., -1.),
        ],
        Stroke::new(1., p.muted),
    ));
}

pub(crate) fn tree_row(ui: &mut egui::Ui, icon: Icon, label: &str, indent: f32) {
    let p = Palette::new(ui.visuals().dark_mode);
    let (row, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.), egui::Sense::hover());
    if indent > 0. {
        let x = row.left() + indent - 12.;
        ui.painter().line_segment(
            [egui::pos2(x, row.top() - 4.), egui::pos2(x, row.center().y)],
            Stroke::new(1., p.border),
        );
        ui.painter().line_segment(
            [
                egui::pos2(x, row.center().y),
                egui::pos2(x + 8., row.center().y),
            ],
            Stroke::new(1., p.border),
        );
    }
    let r = egui::Rect::from_center_size(
        egui::pos2(row.left() + indent + 9., row.center().y),
        Vec2::splat(18.),
    );
    if matches!(icon, Icon::Folder) {
        ui.painter().rect_filled(
            r.shrink2(Vec2::new(0., 3.)),
            2,
            Color32::from_rgb(255, 201, 66),
        );
    }
    draw_icon(
        ui.painter(),
        r,
        icon,
        if matches!(icon, Icon::Folder) {
            Color32::from_rgb(222, 168, 43)
        } else {
            p.text
        },
    );
    ui.painter().text(
        egui::pos2(r.right() + 10., row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.),
        p.text,
    );
}
