//! The download panel. Port of widget_youtube.py.

use crate::bootstrap::Bootstrap;
use crate::config::Config;
use crate::job::{Job, Spec, State};
use crate::theme::{self, Palette};
use std::path::PathBuf;

/// Our title bar. Fixed height so its drag region is a known band.
const HEADER_H: f32 = 60.0;
const HEADER_MARGIN: egui::Margin = egui::Margin {
    left: 16,
    right: 16,
    top: 10,
    bottom: 10,
};

enum Action {
    Stop(usize),
    Restart(usize),
    Remove(usize),
    RemoveFinished,
    /// Open the finished file, or show it in Explorer.
    Open(usize, bool),
}

pub struct App {
    cfg: Config,
    typoi: Vec<(String, Vec<String>)>,
    output: Vec<(String, Vec<String>)>,
    sel_typoi: String,
    sel_output: String,
    thumbnails: bool,
    metadata: bool,
    save_path: String,
    urls: String,
    jobs: Vec<Job>,
    /// How many downloads may run at once; 0 is no limit. [General] max_jobs.
    max_jobs: usize,
    tools: Bootstrap,
    dnd: Option<crate::dnd::Dnd>,
    taskbar: Option<crate::taskbar::Taskbar>,
    message: Option<String>,
    /// Name of the active theme, one of theme::THEMES. [General] theme.
    theme: String,
    logo: Option<egui::TextureHandle>,
    about_open: bool,
    app_update: crate::about::SelfUpdate,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let cfg = Config::load();
        let save_path = cfg.get("General", "save_path");
        // Same guard as widget_youtube.py: a stale path is cleared, not shown.
        let save_path = if PathBuf::from(&save_path).is_dir() {
            save_path
        } else {
            String::new()
        };

        // An old ini says "dark" or "light"; Palette::get takes either.
        let theme = theme::Palette::get(&cfg.get("General", "theme")).name.to_string();
        theme::install_fonts(ctx);
        theme::apply(ctx, &theme);

        let logo = crate::icon().map(|icon| {
            let image: egui::ColorImage = (&icon).into();
            ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
        });

        // Fetches whatever is missing and updates yt-dlp, off the UI thread.
        let tools = Bootstrap::start(cfg.get_bool("General", "check_updates"), {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        });

        Self {
            typoi: cfg.args_map("Typoi"),
            output: cfg.args_map("Output"),
            sel_typoi: cfg.get("General", "typoi"),
            sel_output: cfg.get("General", "output"),
            thumbnails: cfg.get_bool("General", "thubnails"),
            metadata: cfg.get_bool("General", "metadata"),
            save_path,
            urls: String::new(),
            jobs: Vec::new(),
            max_jobs: cfg.get("General", "max_jobs").trim().parse().unwrap_or(3),
            tools,
            dnd: None,
            taskbar: None,
            message: None,
            theme,
            logo,
            about_open: false,
            app_update: crate::about::SelfUpdate::new(cfg.get_bool("General", "check_updates"), ctx),
            cfg,
        }
    }

    /// The ini's colour for a state, fitted to the active theme.
    fn color(&self, state: State) -> egui::Color32 {
        let [r, g, b] = self.cfg.color(state.ini_key());
        theme::state_color(
            egui::Color32::from_rgb(r, g, b),
            &Palette::get(&self.theme),
        )
    }

    fn set_theme(&mut self, ctx: &egui::Context, name: &str) {
        self.theme = name.to_owned();
        theme::apply(ctx, name);
        self.cfg.set("General", "theme", self.theme.clone());
        self.cfg.save();
    }

    /// Start the freshly installed exe, then close this one.
    fn restart(&mut self, ctx: &egui::Context) {
        self.save_settings();
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).spawn();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn repaint(ctx: &egui::Context) -> impl Fn() + Send + Clone + 'static {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    }

    /// Where the window last was, in physical pixels: [General] window = x,y,w,h.
    pub fn saved_window(&self) -> Option<[i32; 4]> {
        let v: Vec<i32> = self
            .cfg
            .get("General", "window")
            .split(',')
            .map(|n| n.trim().parse().ok())
            .collect::<Option<_>>()?;
        v.try_into().ok()
    }

    /// Kept for the next start. Not while maximized or minimized: that is not a
    /// place to come back to, and a minimized window reports -32000,-32000.
    pub fn remember_window(&mut self, window: &winit::window::Window) {
        if window.is_maximized() || window.is_minimized() == Some(true) {
            return;
        }
        if let Ok(pos) = window.outer_position() {
            let size = window.inner_size();
            self.cfg.set(
                "General",
                "window",
                format!("{},{},{},{}", pos.x, pos.y, size.width, size.height),
            );
        }
    }

    fn save_settings(&mut self) {
        self.cfg.set_bool("General", "thubnails", self.thumbnails);
        self.cfg.set_bool("General", "metadata", self.metadata);
        self.cfg.set("General", "typoi", self.sel_typoi.clone());
        self.cfg.set("General", "output", self.sel_output.clone());
        self.cfg.set("General", "save_path", self.save_path.clone());
        self.cfg
            .set("General", "theme", self.theme.clone());
        self.cfg.save();
    }

    /// Port of create_parameters(): output template, format args, then extras.
    fn parameters(&self, typoi: &str) -> Vec<String> {
        let pick = |list: &Vec<(String, Vec<String>)>, key: &str| {
            list.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let mut args = pick(&self.output, &self.sel_output);
        args.extend(pick(&self.typoi, typoi));
        if self.thumbnails {
            args.push("--embed-thumbnail".into());
            args.push("--postprocessor-args".into());
            args.push("-id3v2_version 3".into());
        }
        if self.metadata {
            args.push("--add-metadata".into());
        }
        args
    }

    fn check_before_run(&mut self) -> bool {
        if self.cfg.dl_exe.is_none() {
            self.message = Some("yt-dlp.exe not found next to rficus.exe.".into());
            return false;
        }
        if self.save_path.is_empty() {
            self.message = Some("Save path not set.".into());
            return false;
        }
        if !PathBuf::from(&self.save_path).is_dir() {
            self.message = Some(format!("Save path does not exist:\n{}", self.save_path));
            return false;
        }
        true
    }

    fn download(&mut self) {
        self.save_settings();
        if !self.check_before_run() {
            return;
        }
        let exe = self.cfg.dl_exe.clone().unwrap();
        let base = self.parameters(&self.sel_typoi);
        let urls: Vec<String> = self
            .urls
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        for url in urls {
            if self.jobs.iter().any(|j| j.spec.name == url) {
                continue; // same dedup on url as ProcessManager.new_process
            }
            let mut args = base.clone();
            args.push(url.clone());
            // Queued, not started: start_queued() honours the concurrency cap.
            self.jobs.push(Job::new(Spec {
                name: url,
                exe: exe.clone(),
                args,
                cwd: Some(PathBuf::from(&self.save_path)),
            }));
        }
        self.urls.clear();
    }

    /// Re-run the startup check by hand. Always allowed to use the network,
    /// unlike the automatic one: clicking it *is* asking to check.
    fn check_tools(&mut self, ctx: &egui::Context) {
        self.tools = Bootstrap::start(true, {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        });
    }

    fn open_dir(&mut self) {
        if !self.check_before_run() {
            return;
        }
        #[cfg(windows)]
        let _ = std::process::Command::new("explorer")
            .arg(self.save_path.replace('/', "\\"))
            .spawn();
    }

    /// Open a job's file with whatever handles it, or select it in Explorer.
    /// Falls back to the download folder while there is no file yet.
    fn open_file(&mut self, i: usize, reveal: bool) {
        let path = PathBuf::from(&self.save_path).join(&self.jobs[i].filename);
        if self.jobs[i].filename.is_empty() || !path.exists() {
            self.open_dir();
            return;
        }
        #[cfg(windows)]
        {
            let path = path.display().to_string().replace('/', "\\");
            let mut cmd = std::process::Command::new("explorer");
            if reveal {
                // One argument: explorer wants `/select,<path>` unsplit.
                cmd.arg(format!("/select,{path}"));
            } else {
                cmd.arg(path);
            }
            let _ = cmd.spawn();
        }
    }

    fn pick_save_path(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(&self.save_path)
            .pick_folder()
        {
            self.save_path = dir.display().to_string();
            self.cfg.set("General", "save_path", self.save_path.clone());
            self.cfg.save();
        }
    }

    /// Drops arrive through our own OLE target (see `dnd`), because winit's
    /// accepts files only. Registered on the first frame, once there is a window.
    fn handle_drops(&mut self, ctx: &egui::Context, window: &impl raw_window_handle::HasWindowHandle) {
        if self.dnd.is_none() {
            use raw_window_handle::RawWindowHandle;
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                let ctx = ctx.clone();
                self.dnd = crate::dnd::Dnd::install(win32.hwnd.get(), move || {
                    ctx.request_repaint()
                });
                // Same window handle, same first frame.
                self.taskbar = crate::taskbar::Taskbar::new(win32.hwnd.get());
            }
        }

        let Some(dnd) = &self.dnd else { return };
        for url in dnd.take() {
            self.urls.push_str(&url);
            self.urls.push('\n');
        }
    }

    /// Resize by the window edges, which an undecorated window does not get
    /// from the OS. A 6px band inside each edge — where our panels have only
    /// margin, never a widget — shows the resize cursor and hands the drag to
    /// the window manager.
    fn edge_resize(ctx: &egui::Context) {
        use egui::{CursorIcon, ResizeDirection as D};
        const GRAB: f32 = 6.0;

        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let Some(pos) = ctx.pointer_latest_pos() else {
            return;
        };
        let r = ctx.input(|i| i.viewport_rect());
        let (w, e) = (pos.x <= r.left() + GRAB, pos.x >= r.right() - GRAB);
        let (n, s) = (pos.y <= r.top() + GRAB, pos.y >= r.bottom() - GRAB);
        let dir = match (n, s, w, e) {
            (true, _, true, _) => Some((D::NorthWest, CursorIcon::ResizeNorthWest)),
            (true, _, _, true) => Some((D::NorthEast, CursorIcon::ResizeNorthEast)),
            (_, true, true, _) => Some((D::SouthWest, CursorIcon::ResizeSouthWest)),
            (_, true, _, true) => Some((D::SouthEast, CursorIcon::ResizeSouthEast)),
            (true, ..) => Some((D::North, CursorIcon::ResizeNorth)),
            (_, true, ..) => Some((D::South, CursorIcon::ResizeSouth)),
            (_, _, true, _) => Some((D::West, CursorIcon::ResizeWest)),
            (_, _, _, true) => Some((D::East, CursorIcon::ResizeEast)),
            _ => None,
        };
        if let Some((dir, cursor)) = dir {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            }
        }
    }

    fn url_count(&self) -> usize {
        self.urls.lines().filter(|l| !l.trim().is_empty()).count()
    }

    fn header(&mut self, ui: &mut egui::Ui, p: &Palette, ctx: &egui::Context) {
        // Our title bar: dragging it moves the window, double-clicking it
        // maximises. Claimed before the contents are drawn, so the buttons and
        // the theme picker on top of it still get their own clicks.
        let bar = ui.interact(
            // Out to the frame's margins: the whole bar drags, not just the
            // strip the widgets happen to occupy.
            ui.max_rect().expand2(egui::Vec2::new(
                HEADER_MARGIN.left as f32,
                HEADER_MARGIN.top as f32,
            )),
            ui.id().with("titlebar"),
            egui::Sense::click_and_drag(),
        );
        if bar.double_clicked() {
            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        } else if bar.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // The controls on the right claim their width first; the name and the
        // tool status then live in whatever is left and truncate into it. Laid
        // out the other way round they keep their full width, and in a narrow
        // window the buttons are simply drawn on top of them.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Rightmost first in this layout: close, then maximise, then minimise.
            if theme::window_button(ui, theme::WinButton::Close, p)
                .on_hover_text("Close")
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            let kind = if maximized {
                theme::WinButton::Restore
            } else {
                theme::WinButton::Maximize
            };
            if theme::window_button(ui, kind, p)
                .on_hover_text(if maximized { "Restore" } else { "Maximize" })
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if theme::window_button(ui, theme::WinButton::Minimize, p)
                .on_hover_text("Minimize")
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            ui.add_space(8.0);

            let about = ui
                .add(egui::Button::new("About").min_size(egui::vec2(76.0, 0.0)))
                .on_hover_text("About rficus, theme and updates");
            if self.app_update.has_news() {
                // A badge on the corner: there is an update waiting in there.
                ui.painter().circle(
                    about.rect.right_top() + egui::vec2(-4.0, 4.0),
                    5.0,
                    p.accent,
                    egui::Stroke::new(2.0, p.panel),
                );
            }
            if about.clicked() {
                self.about_open = true;
            }

            if ui
                .add_enabled(!self.tools.busy, egui::Button::new("Check tools"))
                .on_hover_text(
                    "Re-check yt-dlp and ffmpeg, and update yt-dlp if a newer\n\
                     release exists. Runs automatically at startup.",
                )
                .clicked()
            {
                self.check_tools(ctx);
            }

            // Everything still unclaimed, filled from the left.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                if let Some(logo) = &self.logo {
                    ui.add(
                        egui::Image::new(logo)
                            .fit_to_exact_size(egui::Vec2::splat(26.0))
                            .corner_radius(egui::CornerRadius::same(6)),
                    );
                }
                ui.add_space(2.0);
                // Truncating, not wrapping: a narrow window shortens the name
                // and the status rather than growing the bar.
                // The labels truncate into whatever width they are given, so
                // hold back room for the bar or it lands on "Check tools".
                const BAR: f32 = 150.0;
                let bar_room = if self.tools.busy { BAR + 6.0 + ui.spacing().item_spacing.x } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - bar_room).max(0.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("rficus").heading().strong().color(p.text),
                        )
                        .truncate(),
                    );
                    let color = if self.tools.problems.is_empty() {
                        p.dim
                    } else {
                        self.color(State::Error)
                    };
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.tools.status).small().color(color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.tools.status);
                });

                if self.tools.busy {
                    ui.add_space(6.0);
                    let bar = match self.tools.progress {
                        Some(pct) => egui::ProgressBar::new(pct / 100.0)
                            .text(egui::RichText::new(format!("{pct:.0}%")).small()),
                        // No content-length: a moving bar still says "alive".
                        None => egui::ProgressBar::new(0.0).animate(true),
                    };
                    ui.add(
                        bar.desired_width(BAR)
                            .desired_height(14.0)
                            .corner_radius(egui::CornerRadius::ZERO),
                    );
                }
            });
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui, p: &Palette) {
        theme::card(p).show(ui, |ui| {
            ui.label(egui::RichText::new("URLS").small().strong().color(p.dim));
            ui.add(
                egui::TextEdit::multiline(&mut self.urls)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Drag a link here, or paste one url per line"),
            );

        });

        ui.add_space(8.0);

        theme::card(p).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Format").small().strong().color(p.dim));
                egui::ComboBox::from_id_salt("typoi")
                    .selected_text(&self.sel_typoi)
                    .show_ui(ui, |ui| {
                        for (key, _) in &self.typoi {
                            ui.selectable_value(&mut self.sel_typoi, key.clone(), key);
                        }
                    })
                    .response
                    .on_hover_text("File format (video, mp3)");

                ui.add_space(6.0);
                ui.label(egui::RichText::new("Name").small().strong().color(p.dim));
                egui::ComboBox::from_id_salt("output")
                    .selected_text(&self.sel_output)
                    .show_ui(ui, |ui| {
                        for (key, _) in &self.output {
                            ui.selectable_value(&mut self.sel_output, key.clone(), key);
                        }
                    })
                    .response
                    .on_hover_text("Filename template");

                ui.add_space(6.0);
                ui.checkbox(&mut self.thumbnails, "thumbnail")
                    .on_hover_text("Embed the video thumbnail in the file");
                ui.checkbox(&mut self.metadata, "metadata")
                    .on_hover_text("Add metadata\nBe warned: sometimes blocks downloading");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.url_count();
                    let label = match n {
                        0 | 1 => "Download".to_string(),
                        n => format!("Download {n}"),
                    };
                    let enabled = n > 0;
                    let fill = if enabled {
                        p.accent
                    } else {
                        p.accent.gamma_multiply(0.35)
                    };
                    let button = egui::Button::new(
                        egui::RichText::new(label)
                            .strong()
                            .color(theme::on(p.accent)),
                    )
                    .fill(fill)
                    .min_size(egui::Vec2::new(120.0, 30.0));
                    let hover = if enabled {
                        "Start downloading"
                    } else {
                        "Add a url first"
                    };
                    if ui
                        .add_enabled(enabled, button)
                        .on_hover_text(hover)
                        .clicked()
                    {
                        self.download();
                    }
                });
            });
        });

        ui.add_space(8.0);

        theme::card(p).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Save to").small().strong().color(p.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Browse")
                        .on_hover_text("Choose the download folder")
                        .clicked()
                    {
                        self.pick_save_path();
                    }
                    if ui
                        .button("Open")
                        .on_hover_text("Open the download folder")
                        .clicked()
                    {
                        self.open_dir();
                    }
                    let shown = if self.save_path.is_empty() {
                        "not set - click Browse".to_string()
                    } else {
                        self.save_path.clone()
                    };
                    let color = if self.save_path.is_empty() {
                        self.color(State::Error)
                    } else {
                        p.text
                    };
                    if ui
                        .add(
                            egui::Label::new(egui::RichText::new(shown).color(color))
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Click to change")
                        .clicked()
                    {
                        self.pick_save_path();
                    }
                });
            });
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, p: &Palette, height: f32) -> Option<Action> {
        let mut action = None;
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .vscroll(true)
            .min_scrolled_height(0.0)
            .max_scroll_height(height)
            .auto_shrink([false, false])
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::exact(96.0))
            .column(egui_extras::Column::exact(90.0))
            .column(egui_extras::Column::initial(240.0).at_least(120.0))
            .column(egui_extras::Column::remainder())
            .header(24.0, |mut header| {
                for name in ["STATE", "SPEED", "FILE", "LOG"] {
                    header.col(|ui| {
                        ui.label(egui::RichText::new(name).small().strong().color(p.dim));
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, self.jobs.len(), |mut row| {
                    let i = row.index();
                    let (state, rate, speeds, filename, name, log) = {
                        let j = &self.jobs[i];
                        (
                            j.state,
                            j.rate.clone(),
                            j.speeds.clone(),
                            j.filename.clone(),
                            j.spec.name.clone(),
                            j.log.clone(),
                        )
                    };
                    let color = self.color(state);

                    row.col(|ui| {
                        theme::pill(ui, state.label(), color, theme::on(color));
                    });
                    row.col(|ui| {
                        let stroke = egui::Stroke::new(
                            1.5,
                            if state == State::Running {
                                p.speed
                            } else {
                                // The same line, gone quiet: a finished row keeps
                                // its shape but stops competing for attention.
                                p.speed.gamma_multiply(0.45)
                            },
                        );
                        let resp = theme::sparkline(ui, &speeds, stroke);
                        if !rate.is_empty() {
                            resp.on_hover_text(&rate);
                        }
                    });
                    row.col(|ui| {
                        let shown = if filename.is_empty() { "…" } else { &filename };
                        ui.add(egui::Label::new(shown).truncate())
                            .on_hover_text(&name);
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&log).small().color(p.dim))
                                .truncate(),
                        )
                        .on_hover_text(&log);
                    });

                    if row.response().double_clicked() {
                        action = Some(Action::Open(i, false));
                    }

                    row.response().context_menu(|ui| {
                        if ui.button("Open file").clicked() {
                            action = Some(Action::Open(i, false));
                            ui.close();
                        }
                        if ui.button("Show in folder").clicked() {
                            action = Some(Action::Open(i, true));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Stop").clicked() {
                            action = Some(Action::Stop(i));
                            ui.close();
                        }
                        if ui.button("Restart").clicked() {
                            action = Some(Action::Restart(i));
                            ui.close();
                        }
                        if ui.button("Remove").clicked() {
                            action = Some(Action::Remove(i));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Remove all finished").clicked() {
                            action = Some(Action::RemoveFinished);
                            ui.close();
                        }
                    });
                });
            });
        action
    }
}

impl App {
    pub fn ui(&mut self, root: &mut egui::Ui, window: &winit::window::Window) {
        let ctx = &root.ctx().clone();
        self.tools.poll();
        if self.tools.just_finished {
            // yt-dlp may not have existed when the app started.
            self.cfg.refresh_dl_exe();
        }
        for job in &mut self.jobs {
            job.poll();
        }
        crate::job::start_queued(&mut self.jobs, self.max_jobs, Self::repaint(ctx));
        self.handle_drops(ctx, window);
        Self::edge_resize(ctx);
        let running = self.jobs.iter().filter(|j| j.is_running()).count();
        let queued = self
            .jobs
            .iter()
            .filter(|j| j.state == State::Queued)
            .count();
        // Overall progress: the mean of what is actually in flight.
        let overall = (running > 0).then(|| {
            self.jobs
                .iter()
                .filter(|j| j.is_running())
                .map(|j| j.percent)
                .sum::<f32>()
                / running as f32
        });
        if let Some(taskbar) = &mut self.taskbar {
            taskbar.set(overall);
        }

        let p = Palette::get(&self.theme);
        let mut action = None;

        // Fixed height, because the title bar's drag region is its whole area:
        // an auto-sized panel reports the rest of the window as its max_rect,
        // and dragging the job table would then move the window.
        egui::Panel::top("header")
            .exact_size(HEADER_H)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(HEADER_MARGIN),
            )
            .show(root, |ui| {
                self.header(ui, &p, ctx);
            });

        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 6)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let total = self.jobs.len();
                    let mut text = match (total, running) {
                        (0, _) => "No jobs".to_string(),
                        (t, 0) => format!("{t} jobs, none running"),
                        (t, r) => format!("{t} jobs, {r} running"),
                    };
                    if queued > 0 {
                        text.push_str(&format!(", {queued} queued"));
                    }
                    if let Some(pct) = overall {
                        text.push_str(&format!(" · {pct:.0}%"));
                    }
                    ui.label(egui::RichText::new(text).small().color(p.dim));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("right-click a row for stop / restart / remove")
                                .small()
                                .color(p.dim),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(root, |ui| {
                self.controls(ui, &p);
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("JOBS").small().strong().color(p.dim));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                self.jobs.iter().any(|j| !j.is_running() && j.state != State::Queued),
                                egui::Button::new(egui::RichText::new("Clear finished").small()),
                            )
                            .clicked()
                        {
                            action = Some(Action::RemoveFinished);
                        }
                    });
                });
                ui.add_space(2.0);

                // The jobs card takes whatever vertical space is left, and the
                // table scrolls inside it rather than running under the status bar.
                let card_h = (ui.available_height() - 4.0).max(80.0);
                theme::card(&p).show(ui, |ui| {
                    ui.set_min_height(card_h - 24.0);
                    ui.set_max_height(card_h - 24.0);
                    if self.jobs.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space((card_h - 90.0).max(8.0) / 2.0);
                            ui.label(egui::RichText::new("Nothing downloading yet").color(p.dim));
                            ui.label(
                                egui::RichText::new("Paste a url above and hit Download")
                                    .small()
                                    .color(p.dim),
                            );
                        });
                    } else if let Some(a) = self.table(ui, &p, card_h - 52.0) {
                        action = Some(a);
                    }
                });
            });

        match action {
            Some(Action::Stop(i)) => self.jobs[i].stop(),
            Some(Action::Restart(i)) => self.jobs[i].requeue(),
            Some(Action::Open(i, reveal)) => self.open_file(i, reveal),
            Some(Action::Remove(i)) => {
                self.jobs.remove(i);
            }
            // Queued jobs have not run yet: clearing "finished" must not eat them.
            Some(Action::RemoveFinished) => self.jobs.retain(|j| j.is_running() || j.state == State::Queued),
            None => {}
        }

        self.app_update.poll();
        if self.about_open {
            let colors = crate::about::Colors {
                ok: self.color(State::Finished),
                error: self.color(State::Error),
            };
            let out = crate::about::show(ctx, &p, self.logo.as_ref(), &mut self.app_update, colors, running > 0);
            if let Some(name) = out.theme {
                self.set_theme(ctx, name);
            }
            if out.restart {
                self.restart(ctx);
            }
            self.about_open = !out.close;
        }

        if let Some(text) = self.message.clone() {
            egui::Window::new("Heads up")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        if ui.button("OK").clicked() {
                            self.message = None;
                        }
                    });
                });
        }

        // Last thing in the frame, because a job can be added *during* it:
        // clicking Download builds its row further down this same pass. Asking
        // at the top counted zero, scheduled nothing, and left egui idle with a
        // live download nobody was polling — the row then sat at "Starting"
        // until some unrelated event happened to wake the UI.
        // A queued job needs a frame to be started in, too.
        if self.jobs.iter().any(Job::needs_poll) || queued > 0 || self.tools.busy {
            // try_wait() also needs a tick to notice an exit that produced no
            // output, and the bootstrap's animated bar needs frames.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    pub fn on_exit(&mut self) {
        self.save_settings();
    }
}
