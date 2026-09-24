//! The About window: what rficus is and who made it, the theme picker, and
//! rficus's own updates. The network half runs on a worker thread, like the
//! tool bootstrap; the window only reads what it reports.

use crate::theme::{self, Palette};
use crate::update;
use egui::{Color32, CornerRadius, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2, vec2};
use std::sync::mpsc::{Receiver, Sender, channel};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const WIDTH: f32 = 540.0;
const HERO_H: f32 = 116.0;
const RADIUS: u8 = 16;

const HELP: &[&str] = &[
    "Paste links into the box, or drag them straight from your browser.",
    "Pick a format and how files are named, then hit Download.",
    "Every link runs on its own. Right-click a row to stop, restart or remove it.",
    "yt-dlp and ffmpeg are fetched and kept current for you. Check tools re-checks them.",
];

pub enum Update {
    /// Automatic checks are off and nobody has asked yet.
    Idle,
    Checking,
    UpToDate,
    Available(String),
    Downloading(String, Option<f32>),
    Installed(String),
    Failed(String),
}

enum Msg {
    Checked(Result<Option<String>, String>),
    Progress(Option<f32>),
    Installed(Result<(), String>),
}

pub struct SelfUpdate {
    rx: Option<Receiver<Msg>>,
    pub state: Update,
}

impl SelfUpdate {
    pub fn new(check: bool, ctx: &egui::Context) -> Self {
        update::remove_old_self();
        let mut s = Self { rx: None, state: Update::Idle };
        if check {
            s.check(ctx);
        }
        s
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        self.state = Update::Checking;
        self.spawn(ctx, |tx, _| {
            let _ = tx.send(Msg::Checked(update::newer_app_version()));
        });
    }

    fn install(&mut self, ctx: &egui::Context) {
        let Update::Available(v) = &self.state else { return };
        let v = v.clone();
        self.state = Update::Downloading(v.clone(), None);
        self.spawn(ctx, move |tx, ctx| {
            let mut last = 0;
            let result = update::replace_self(&v, &mut |done, total| {
                // One message per 256 KB, as the bootstrap does.
                if done - last >= 256 * 1024 || Some(done) == total {
                    last = done;
                    let _ = tx.send(Msg::Progress(total.map(|t| done as f32 / t.max(1) as f32)));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::Installed(result));
        });
    }

    fn spawn(&mut self, ctx: &egui::Context, work: impl FnOnce(&Sender<Msg>, &egui::Context) + Send + 'static) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            work(&tx, &ctx);
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(msg) = rx.try_recv() {
            self.state = match (msg, &self.state) {
                (Msg::Checked(Ok(Some(v))), _) => Update::Available(v),
                (Msg::Checked(Ok(None)), _) => Update::UpToDate,
                (Msg::Checked(Err(e)), _) => Update::Failed(e),
                (Msg::Progress(pct), Update::Downloading(v, _)) => Update::Downloading(v.clone(), pct),
                (Msg::Installed(Ok(())), Update::Downloading(v, _)) => Update::Installed(v.clone()),
                (Msg::Installed(Err(e)), _) => Update::Failed(e),
                _ => continue,
            };
        }
    }

    /// Worth a badge on the About button.
    pub fn has_news(&self) -> bool {
        matches!(self.state, Update::Available(_) | Update::Installed(_))
    }
}

#[derive(Default)]
pub struct Outcome {
    pub close: bool,
    pub restart: bool,
    pub theme: Option<&'static str>,
}

pub struct Colors {
    pub ok: Color32,
    pub error: Color32,
}

pub fn show(
    ctx: &egui::Context,
    p: &Palette,
    logo: Option<&egui::TextureHandle>,
    upd: &mut SelfUpdate,
    colors: Colors,
    downloads_running: bool,
) -> Outcome {
    let mut out = Outcome::default();
    let frame = egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(RADIUS))
        .shadow(egui::Shadow {
            offset: [0, 14],
            blur: 44,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 150 } else { 60 }),
        });
    let modal = egui::Modal::new(egui::Id::new("about"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if p.dark { 140 } else { 90 }))
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            if hero(ui, p, logo) {
                out.close = true;
            }
            // Short windows scroll the body; the hero stays put. Sized from the
            // window, not from what is left below the modal: on the first frame
            // the modal does not know its own height yet, sits too low, and
            // would lock the body into a stub.
            let max_h = (ctx.content_rect().height() - HERO_H - 40.0 - 48.0).max(160.0);
            egui::Frame::new().inner_margin(Margin::same(20)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).show(ui, |ui| {
                    ui.set_width(WIDTH - 40.0);
                    update_card(ui, p, upd, &colors, downloads_running, &mut out);
                    ui.add_space(14.0);
                    section(ui, p, "THEME");
                    themes(ui, p, &mut out);
                    ui.add_space(14.0);
                    section(ui, p, "QUICK START");
                    help(ui, p);
                    ui.add_space(10.0);
                    footer(ui, p);
                });
            });
        });
    out.close |= modal.should_close();
    out
}

/// The accent band across the top: logo, name, version, and the close button.
/// Returns true when the close button was clicked.
fn hero(ui: &mut egui::Ui, p: &Palette, logo: Option<&egui::TextureHandle>) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HERO_H), Sense::hover());
    let painter = ui.painter_at(rect);
    let fg = theme::on(p.accent);
    let radius = CornerRadius { nw: RADIUS, ne: RADIUS, sw: 0, se: 0 };
    painter.rect_filled(rect, radius, p.accent);
    // Soft light blobs for depth. Kept clear of the rounded top corners, which
    // the clip rect would not round off.
    let glow = |a: f32| fg.gamma_multiply(a);
    painter.circle_filled(rect.right_top() + vec2(-86.0, 12.0), 64.0, glow(0.10));
    painter.circle_filled(rect.right_bottom() + vec2(-200.0, 34.0), 58.0, glow(0.07));
    painter.circle_filled(rect.left_bottom() + vec2(150.0, 46.0), 30.0, glow(0.06));

    // Logo on a white tile, so it reads on any accent.
    let tile = Rect::from_min_size(rect.left_top() + vec2(24.0, (HERO_H - 72.0) / 2.0), Vec2::splat(72.0));
    painter.rect_filled(tile, CornerRadius::same(18), Color32::from_white_alpha(240));
    if let Some(logo) = logo {
        egui::Image::new(logo)
            .corner_radius(CornerRadius::same(12))
            .paint_at(ui, tile.shrink(10.0));
    }

    let x = tile.right() + 20.0;
    let name = painter.text(
        Pos2::new(x, rect.center().y - 6.0),
        egui::Align2::LEFT_BOTTOM,
        "rficus",
        egui::FontId::proportional(30.0),
        fg,
    );
    // Version pill beside the name.
    let galley = painter.layout_no_wrap(format!("v{VERSION}"), egui::FontId::proportional(12.0), fg);
    let pill = Rect::from_min_size(
        Pos2::new(name.right() + 10.0, name.center().y - 9.0),
        vec2(galley.size().x + 16.0, 20.0),
    );
    painter.rect_filled(pill, CornerRadius::same(10), glow(0.20));
    painter.galley(pill.center() - galley.size() / 2.0, galley, fg);
    painter.text(
        Pos2::new(x, rect.center().y + 4.0),
        egui::Align2::LEFT_TOP,
        "A friendly front-end for yt-dlp",
        egui::FontId::proportional(14.5),
        fg.gamma_multiply(0.85),
    );

    // Close: a round ghost button in the corner.
    let close = Rect::from_center_size(rect.right_top() + vec2(-26.0, 26.0), Vec2::splat(30.0));
    let resp = ui
        .interact(close, ui.id().with("about_close"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Close (Esc)");
    if resp.hovered() {
        painter.circle_filled(close.center(), 15.0, glow(0.18));
    }
    let d = 5.0;
    let c = close.center();
    let stroke = Stroke::new(1.6, fg);
    painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
    painter.line_segment([c + vec2(d, -d), c + vec2(-d, d)], stroke);
    resp.clicked()
}

fn section(ui: &mut egui::Ui, p: &Palette, title: &str) {
    ui.label(RichText::new(title).small().strong().color(p.dim));
}

fn primary(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(theme::on(p.accent)))
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(8))
            .min_size(vec2(104.0, 32.0)),
    )
}

fn update_card(
    ui: &mut egui::Ui,
    p: &Palette,
    upd: &mut SelfUpdate,
    colors: &Colors,
    downloads_running: bool,
    out: &mut Outcome,
) {
    let ctx = ui.ctx().clone();
    theme::card(p).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (dot, title, detail) = match &upd.state {
            Update::Idle => (p.dim, "Updates".to_owned(), "Automatic checks are off".to_owned()),
            Update::Checking => (p.dim, "Checking for updates…".to_owned(), format!("You have {VERSION}")),
            Update::UpToDate => (colors.ok, "You're up to date".to_owned(), format!("{VERSION} is the latest release")),
            Update::Available(v) => (p.accent, format!("rficus {v} is available"), format!("You have {VERSION}")),
            Update::Downloading(v, _) => (p.accent, format!("Downloading {v}…"), "The app restarts into it when you are ready".to_owned()),
            Update::Installed(v) => (
                colors.ok,
                format!("Updated to {v}"),
                if downloads_running {
                    "Restart once your downloads finish".to_owned()
                } else {
                    "Restart to start using it".to_owned()
                },
            ),
            Update::Failed(e) => (colors.error, "Update failed".to_owned(), e.clone()),
        };
        ui.horizontal(|ui| {
            if matches!(upd.state, Update::Checking) {
                ui.add(egui::Spinner::new().size(14.0).color(p.accent));
            } else {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 7.0, dot.gamma_multiply(0.22));
                ui.painter().circle_filled(r.center(), 4.0, dot);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(title).color(p.text));
                ui.add(egui::Label::new(RichText::new(detail).small().color(p.dim)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match &upd.state {
                Update::Available(_) => {
                    if primary(ui, p, "Upgrade").clicked() {
                        upd.install(&ctx);
                    }
                }
                Update::Installed(_) => {
                    if ui.add_enabled_ui(!downloads_running, |ui| primary(ui, p, "Restart now")).inner.clicked() {
                        out.restart = true;
                    }
                }
                Update::Idle => {
                    if ui.button("Check now").clicked() {
                        upd.check(&ctx);
                    }
                }
                Update::UpToDate | Update::Failed(_) => {
                    if ui.button(RichText::new("Check again").small()).clicked() {
                        upd.check(&ctx);
                    }
                }
                Update::Checking | Update::Downloading(..) => {}
            });
        });
        if let Update::Downloading(_, pct) = upd.state {
            let bar = match pct {
                Some(f) => egui::ProgressBar::new(f).text(RichText::new(format!("{:.0}%", f * 100.0)).small()),
                None => egui::ProgressBar::new(0.0).animate(true),
            };
            ui.add(bar.desired_height(8.0).fill(p.accent).corner_radius(CornerRadius::same(4)));
        }
    });
}

/// Every theme as a live miniature of itself; a click applies it.
fn themes(ui: &mut egui::Ui, p: &Palette, out: &mut Outcome) {
    const COLS: usize = 5;
    let gap = 10.0;
    let w = (ui.available_width() - gap * (COLS - 1) as f32) / COLS as f32;
    for row in theme::THEMES.chunks(COLS) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for t in row {
                if swatch(ui, t, t.name == p.name, p, w).clicked() && t.name != p.name {
                    out.theme = Some(t.name);
                }
            }
        });
    }
}

fn swatch(ui: &mut egui::Ui, t: &Palette, selected: bool, p: &Palette, w: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 78.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter();
    let lift = if resp.hovered() && !selected { -2.0 } else { 0.0 };
    let preview = Rect::from_min_size(rect.min + vec2(0.0, 2.0 + lift), vec2(w, 56.0));
    let border = if selected {
        Stroke::new(2.0, p.accent)
    } else if resp.hovered() {
        Stroke::new(1.0, p.dim)
    } else {
        Stroke::new(1.0, p.line)
    };
    painter.rect(preview, CornerRadius::same(10), t.bg, border, StrokeKind::Outside);
    // A title-bar strip, then a card with two text lines and an accent button.
    let strip = Rect::from_min_size(preview.min, vec2(w, 12.0));
    painter.rect_filled(strip, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, t.panel);
    let card = Rect::from_min_max(preview.min + vec2(8.0, 18.0), preview.max - vec2(8.0, 8.0));
    painter.rect(card, CornerRadius::same(5), t.card, Stroke::new(1.0, t.line), StrokeKind::Inside);
    let line = |y: f32, frac: f32, c: Color32| {
        let r = Rect::from_min_size(card.min + vec2(7.0, y), vec2((card.width() - 14.0) * frac, 4.0));
        painter.rect_filled(r, CornerRadius::same(2), c);
    };
    line(8.0, 0.55, t.text);
    line(17.0, 0.35, t.dim);
    let button = Rect::from_min_size(card.right_top() + vec2(-26.0, 9.0), vec2(18.0, 12.0));
    painter.rect_filled(button, CornerRadius::same(3), t.accent);

    if selected {
        let c = preview.right_top() + vec2(-2.0, 2.0);
        painter.circle(c, 8.0, p.accent, Stroke::new(2.0, p.panel));
        let fg = theme::on(p.accent);
        painter.line(
            vec![c + vec2(-3.5, 0.0), c + vec2(-1.0, 2.5), c + vec2(3.5, -2.5)],
            Stroke::new(1.6, fg),
        );
    }
    painter.text(
        Pos2::new(rect.center().x, rect.bottom()),
        egui::Align2::CENTER_BOTTOM,
        t.name,
        egui::FontId::proportional(12.0),
        if selected { p.text } else { p.dim },
    );
    resp
}

fn help(ui: &mut egui::Ui, p: &Palette) {
    for (i, text) in HELP.iter().enumerate() {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 11.0, p.accent.gamma_multiply(0.16));
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                (i + 1).to_string(),
                egui::FontId::proportional(12.0),
                p.accent,
            );
            ui.add(egui::Label::new(RichText::new(*text).color(p.text)).wrap());
        });
    }
}

fn footer(ui: &mut egui::Ui, p: &Palette) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Made by").small().color(p.dim));
        ui.label(RichText::new("Ted Lazaros").small().color(p.text));
        ui.label(RichText::new("·  yt-dlp · ffmpeg · egui").small().color(p.dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let link = ui
                .add(egui::Button::new(RichText::new("GitHub page").small().color(p.accent)).frame(false))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(update::REPO);
            if link.clicked() {
                let _ = std::process::Command::new("explorer").arg(update::REPO).spawn();
            }
        });
    });
}
