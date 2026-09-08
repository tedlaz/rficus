//! Visual style. egui's stock look is fine for debug tools; this is the app.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Shadow, Stroke, TextStyle, Vec2,
};

#[derive(Clone, Copy)]
pub struct Palette {
    pub name: &'static str,
    pub dark: bool,
    pub accent: Color32,
    /// The download speed line. Warm in every theme — it is the one reading
    /// that is measured rather than estimated — but tuned per palette, bright
    /// on the dark ones and deepened on the light ones so it stays legible.
    pub speed: Color32,
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub field: Color32,
    pub line: Color32,
    pub text: Color32,
    pub dim: Color32,
}

/// Every theme on offer, in the order the picker lists them. The first is the
/// fallback for an unknown name in the ini.
pub const THEMES: &[Palette] = &[
    Palette {
        name: "Dark",
        dark: true,
        accent: Color32::from_rgb(88, 132, 255),
        speed: Color32::from_rgb(245, 158, 32),
        bg: Color32::from_rgb(20, 22, 28),
        panel: Color32::from_rgb(26, 29, 36),
        card: Color32::from_rgb(33, 37, 46),
        card_hover: Color32::from_rgb(42, 47, 58),
        field: Color32::from_rgb(16, 18, 23),
        line: Color32::from_rgb(48, 53, 65),
        text: Color32::from_rgb(226, 230, 238),
        dim: Color32::from_rgb(138, 146, 163),
    },
    Palette {
        name: "Midnight",
        dark: true,
        speed: Color32::from_rgb(255, 183, 77),
        accent: Color32::from_rgb(149, 117, 255),
        bg: Color32::from_rgb(15, 18, 34),
        panel: Color32::from_rgb(20, 24, 44),
        card: Color32::from_rgb(26, 31, 54),
        card_hover: Color32::from_rgb(35, 41, 68),
        field: Color32::from_rgb(12, 15, 29),
        line: Color32::from_rgb(42, 48, 78),
        text: Color32::from_rgb(223, 227, 245),
        dim: Color32::from_rgb(132, 140, 175),
    },
    Palette {
        name: "Graphite",
        speed: Color32::from_rgb(255, 145, 40),
        dark: true,
        accent: Color32::from_rgb(0, 183, 163),
        bg: Color32::from_rgb(26, 26, 26),
        panel: Color32::from_rgb(32, 32, 32),
        card: Color32::from_rgb(40, 40, 40),
        card_hover: Color32::from_rgb(52, 52, 52),
        field: Color32::from_rgb(22, 22, 22),
        line: Color32::from_rgb(58, 58, 58),
        text: Color32::from_rgb(232, 230, 226),
        dim: Color32::from_rgb(150, 148, 143),
    },
    Palette {
        speed: Color32::from_rgb(205, 124, 0),
        name: "Light",
        dark: false,
        accent: Color32::from_rgb(88, 132, 255),
        bg: Color32::from_rgb(246, 247, 250),
        panel: Color32::from_rgb(255, 255, 255),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(238, 241, 247),
        field: Color32::from_rgb(250, 251, 253),
        line: Color32::from_rgb(220, 224, 232),
        text: Color32::from_rgb(28, 32, 40),
        dim: Color32::from_rgb(112, 120, 136),
    },
    Palette {
        name: "Paper",
        dark: false,
        accent: Color32::from_rgb(184, 100, 30),
        bg: Color32::from_rgb(247, 243, 233),
        panel: Color32::from_rgb(253, 250, 243),
        card: Color32::from_rgb(255, 253, 247),
        card_hover: Color32::from_rgb(240, 234, 221),
        field: Color32::from_rgb(252, 249, 241),
        line: Color32::from_rgb(224, 215, 197),
        text: Color32::from_rgb(45, 40, 32),
        dim: Color32::from_rgb(124, 113, 96),
        speed: Color32::from_rgb(191, 140, 0),
    },
    Palette {
        name: "Mist",
        dark: false,
        accent: Color32::from_rgb(0, 133, 143),
        bg: Color32::from_rgb(240, 244, 248),
        panel: Color32::from_rgb(250, 252, 255),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(231, 238, 246),
        field: Color32::from_rgb(247, 250, 253),
        line: Color32::from_rgb(210, 220, 232),
        text: Color32::from_rgb(27, 38, 50),
        speed: Color32::from_rgb(214, 124, 0),
        dim: Color32::from_rgb(104, 120, 138),
    },
    Palette {
        name: "Solar",
        dark: false,
        accent: Color32::from_rgb(38, 139, 210),
        bg: Color32::from_rgb(253, 246, 227),
        panel: Color32::from_rgb(255, 251, 240),
        card: Color32::from_rgb(255, 252, 242),
        card_hover: Color32::from_rgb(238, 232, 213),
        field: Color32::from_rgb(255, 253, 246),
        line: Color32::from_rgb(231, 223, 201),
        speed: Color32::from_rgb(181, 137, 0),
        text: Color32::from_rgb(24, 54, 62),
        dim: Color32::from_rgb(112, 130, 136),
    },
    Palette {
        name: "Rose",
        dark: false,
        accent: Color32::from_rgb(190, 24, 93),
        bg: Color32::from_rgb(253, 242, 245),
        panel: Color32::from_rgb(255, 250, 251),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(250, 232, 238),
        field: Color32::from_rgb(255, 252, 253),
        speed: Color32::from_rgb(198, 116, 0),
        line: Color32::from_rgb(240, 214, 222),
        text: Color32::from_rgb(60, 32, 42),
        dim: Color32::from_rgb(148, 110, 124),
    },
    Palette {
        name: "Sage",
        dark: false,
        accent: Color32::from_rgb(46, 125, 50),
        bg: Color32::from_rgb(242, 247, 242),
        panel: Color32::from_rgb(251, 254, 251),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(231, 242, 233),
        speed: Color32::from_rgb(200, 120, 0),
        field: Color32::from_rgb(249, 252, 249),
        line: Color32::from_rgb(213, 228, 215),
        text: Color32::from_rgb(30, 45, 34),
        dim: Color32::from_rgb(104, 126, 110),
    },
    Palette {
        name: "Linen",
        dark: false,
        accent: Color32::from_rgb(120, 92, 200),
        speed: Color32::from_rgb(190, 120, 20),
        bg: Color32::from_rgb(245, 244, 240),
        panel: Color32::from_rgb(252, 252, 250),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(237, 236, 230),
        field: Color32::from_rgb(250, 250, 247),
        line: Color32::from_rgb(222, 220, 212),
        text: Color32::from_rgb(38, 37, 34),
        dim: Color32::from_rgb(122, 119, 111),
    },
];

impl Palette {
    /// By name, case-insensitively — an old ini says `dark` or `light`, and the
    /// picker writes `Dark` or `Light`. Anything unrecognised gets the first.
    pub fn get(name: &str) -> Self {
        *THEMES
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name.trim()))
            .unwrap_or(&THEMES[0])
    }
}

/// A card: rounded surface with a hairline border, for grouping controls.
pub fn card(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 12))
}

/// Use the fonts Windows already has instead of bundling ~1.4 MB of them.
/// Segoe UI ships with every Windows since Vista and covers Greek and Cyrillic
/// better than the bundled Ubuntu; the rest are fallbacks in case it is absent.
///
/// Called once at startup. If somehow none of them load, egui is left with its
/// own (empty, since `default_fonts` is off) set rather than a broken one.
pub fn install_fonts(ctx: &egui::Context) {
    let dir = std::path::PathBuf::from(
        std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_owned()),
    )
    .join("Fonts");

    let load = |fonts: &mut egui::FontDefinitions, names: &[(&str, &str)]| -> Vec<String> {
        let mut loaded = Vec::new();
        for (name, file) in names {
            if let Ok(bytes) = std::fs::read(dir.join(file)) {
                fonts.font_data.insert(
                    (*name).to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                loaded.push((*name).to_owned());
            }
        }
        loaded
    };

    let mut fonts = egui::FontDefinitions::empty();
    let proportional = load(
        &mut fonts,
        &[
            ("segoe", "segoeui.ttf"),
            ("tahoma", "tahoma.ttf"),
            ("arial", "arial.ttf"),
            ("verdana", "verdana.ttf"),
        ],
    );
    if proportional.is_empty() {
        eprintln!("no system font found in {}", dir.display());
        return;
    }
    let mut monospace = load(
        &mut fonts,
        &[("consolas", "consola.ttf"), ("courier", "cour.ttf")],
    );
    monospace.extend(proportional.clone());

    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Monospace, monospace);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, name: &str) {
    let p = Palette::get(name);
    let theme = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);

    let accent = p.accent;
    let mut style = (*ctx.style_of(theme)).clone();

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.window_margin = Margin::same(12);

    style.text_styles = [
        (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();

    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.field;
    v.text_edit_bg_color = Some(p.field);
    v.faint_bg_color = if p.dark {
        Color32::from_rgb(28, 31, 39)
    } else {
        Color32::from_rgb(248, 249, 252)
    };
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }),
    };
    v.popup_shadow = v.window_shadow;
    v.selection.bg_fill = accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.text);
    v.hyperlink_color = accent;
    v.override_text_color = Some(p.text);

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.dim);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.card;
    w.inactive.weak_bg_fill = p.card;
    w.inactive.bg_stroke = Stroke::new(1.0, p.line);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.card_hover;
    w.hovered.weak_bg_fill = p.card_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = accent.gamma_multiply(0.35);
    w.active.weak_bg_fill = accent.gamma_multiply(0.35);
    w.active.bg_stroke = Stroke::new(1.0, accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.card_hover;
    w.open.weak_bg_fill = p.card_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.line);
    w.open.corner_radius = radius;

    style.visuals = v;
    ctx.set_style_of(theme, style);
}

/// Filled pill used for state badges and the primary action.
pub fn pill(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        TextStyle::Small.resolve(ui.style()),
        fg,
    );
    let size = Vec2::new(galley.size().x + 18.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), fill);
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, fg);
    }
    response
}

/// A mini line graph of `values`, scaled to their own peak: the shape is the
/// information, not the height. Newest sample on the right, so a short history
/// grows in from the left instead of stretching across the cell.
pub fn sparkline(ui: &mut egui::Ui, values: &[f32], stroke: Stroke) -> egui::Response {
    let size = Vec2::new(ui.available_width(), 14.0);
    // Hover only: a click belongs to the table row underneath.
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if values.len() < 2 || !ui.is_rect_visible(rect) {
        return response;
    }

    let peak = values.iter().copied().fold(0.0f32, f32::max).max(1.0);
    let step = rect.width() / 120.0;
    let right = rect.right();
    let points: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = right - (values.len() - 1 - i) as f32 * step;
            Pos2::new(x, rect.bottom() - (v / peak) * (rect.height() - 2.0))
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, stroke));
    response
}

/// The three window controls of our own title bar.
#[derive(Clone, Copy, PartialEq)]
pub enum WinButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// A window control, drawn rather than typed: the box-drawing glyphs Windows
/// uses for these live in Segoe MDL2 Assets, which we do not load, and a
/// missing glyph would render as a tofu box.
pub fn window_button(ui: &mut egui::Ui, kind: WinButton, p: &Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(38.0, 26.0), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let hovered = response.hovered();
    let fg = if hovered && kind == WinButton::Close {
        Color32::WHITE
    } else {
        p.text
    };
    if hovered {
        let bg = if kind == WinButton::Close {
            Color32::from_rgb(232, 17, 35)
        } else {
            p.card_hover
        };
        ui.painter().rect_filled(rect, CornerRadius::same(4), bg);
    }

    let painter = ui.painter();
    let stroke = Stroke::new(1.2, fg);
    let c = rect.center();
    let r = 5.0;
    match kind {
        WinButton::Minimize => {
            painter.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], stroke);
        }
        WinButton::Maximize => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::splat(r * 2.0)),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        WinButton::Restore => {
            // Two overlapping frames, the back one peeking out top-right.
            let front = egui::Rect::from_min_size(
                Pos2::new(c.x - r, c.y - r + 2.0),
                Vec2::splat(r * 2.0 - 2.0),
            );
            painter.rect_stroke(front, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(c.x - r + 2.0, c.y - r),
                    Pos2::new(c.x + r, c.y - r),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + r, c.y - r),
                    Pos2::new(c.x + r, c.y + r - 2.0),
                ],
                stroke,
            );
        }
        WinButton::Close => {
            painter.line_segment(
                [Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x + r, c.y - r), Pos2::new(c.x - r, c.y + r)],
                stroke,
            );
        }
    }
    response
}

/// A job-state colour, fitted to the theme.
///
/// The `[Tprocess]` hues are the user's and are kept — but they are pastels
/// picked against a dark background, and on a light theme they bleach into the
/// card. There they are deepened; on a dark theme they are used as written.
pub fn state_color(c: Color32, p: &Palette) -> Color32 {
    if p.dark {
        return c;
    }
    let deepen = |v: u8| (v as f32 * 0.62) as u8;
    Color32::from_rgb(deepen(c.r()), deepen(c.g()), deepen(c.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32
    }

    /// The ini's pastels are meant for a dark card. On a light one they have to
    /// come down far enough to read as a filled badge rather than a smudge.
    #[test]
    fn state_colors_stay_visible_on_every_theme() {
        // The [Tprocess] defaults from ficus.ini.
        let states = [
            Color32::from_rgb(0x97, 0xf5, 0xf5),
            Color32::from_rgb(0xae, 0x90, 0xff),
            Color32::from_rgb(0xff, 0xbb, 0x6c),
            Color32::from_rgb(0xff, 0x6c, 0x6c),
            Color32::from_rgb(0xff, 0x6c, 0xdb),
            Color32::from_rgb(0x7b, 0xe2, 0x63),
        ];

        for p in THEMES {
            for raw in states {
                let c = state_color(raw, p);
                if p.dark {
                    assert_eq!(c, raw, "{}: a dark theme keeps the ini colour", p.name);
                } else {
                    let gap = (luminance(c) - luminance(p.card)).abs();
                    assert!(
                        gap > 60.0,
                        "{}: {:?} is only {gap:.0} from the card",
                        p.name,
                        c
                    );
                }
                // Whatever the fill, the label on it has to be readable.
                let fg = on(c);
                assert!(
                    (luminance(fg) - luminance(c)).abs() > 90.0,
                    "{}: unreadable pill text on {:?}",
                    p.name,
                    c
                );
            }
        }
    }
}

/// Readable text color for a given background.
pub fn on(fill: Color32) -> Color32 {
    let l = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if l > 140.0 {
        Color32::from_rgb(20, 22, 28)
    } else {
        Color32::WHITE
    }
}
