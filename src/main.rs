#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod bootstrap;
mod config;
mod dnd;
mod job;
mod parse;
mod taskbar;
mod theme;
mod ui;
mod update;

use std::{num::NonZeroU32, rc::Rc, time::Instant};

use egui::{ViewportEvent, ViewportId};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    window::{Window, WindowId},
};

pub fn icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../icon.png")).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData { rgba: image.into_raw(), width, height })
}

/// Everything that exists once there is a window.
struct Win {
    window: Rc<Window>,
    surface: softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>,
    state: egui_winit::State,
    info: egui::ViewportInfo,
}

/// eframe's job, minus the GPU: winit for the window, egui_software_backend
/// rasterizes on the CPU, softbuffer blits the pixels.
struct Runner {
    ctx: egui::Context,
    app: ui::App,
    viewport: egui::ViewportBuilder,
    softbuffer: softbuffer::Context<OwnedDisplayHandle>,
    renderer: EguiSoftwareRender,
    win: Option<Win>,
    /// The earliest repaint egui asked for, if it is in the future.
    repaint_at: Option<Instant>,
}

impl Runner {
    fn redraw(&mut self, el: &ActiveEventLoop) {
        let Some(win) = &mut self.win else { return };
        let size = win.window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return; // minimized
        };

        egui_winit::update_viewport_info(&mut win.info, &self.ctx, &win.window, false);
        let mut input = win.state.take_egui_input(&win.window);
        input.viewports.insert(ViewportId::ROOT, win.info.clone());
        let window = win.window.clone();
        let mut out = self.ctx.run_ui(input, |ui| self.app.ui(ui, &*window));

        win.state.handle_platform_output(&win.window, out.platform_output);
        win.info.events.clear();
        if let Some(vp) = out.viewport_output.remove(&ViewportId::ROOT) {
            egui_winit::process_viewport_commands(&self.ctx, &mut win.info, vp.commands, &win.window, &mut Vec::new());
        }
        if win.info.events.contains(&ViewportEvent::Close) {
            self.exit(el);
            return;
        }

        let primitives = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        if win.surface.resize(width, height).is_err() {
            return;
        }
        let Ok(mut buffer) = win.surface.buffer_mut() else { return };
        let [r, g, b, _] = self.ctx.global_style().visuals.panel_fill.to_array();
        buffer.fill(u32::from_be_bytes([0, r, g, b]));
        self.renderer.render(
            &mut BufferMutRef::new(bytemuck::cast_slice_mut(&mut buffer), width.get() as usize, height.get() as usize),
            &primitives,
            &out.textures_delta,
            out.pixels_per_point,
        );
        let _ = buffer.present();
    }

    fn exit(&mut self, el: &ActiveEventLoop) {
        if let Some(win) = &self.win {
            self.app.remember_window(&win.window);
        }
        self.app.on_exit();
        el.exit();
    }

    /// The window where it was last time, if that is still on some monitor:
    /// a monitor unplugged since then must not leave it off-screen.
    fn create_window(&self, el: &ActiveEventLoop) -> Window {
        let mut viewport = self.viewport.clone();
        let saved = self.app.saved_window().filter(|&[x, y, _, _]| {
            // Somewhere near the title bar's left end has to be grabbable.
            let (px, py) = (x + 60, y + 20);
            el.available_monitors().any(|m| {
                let (pos, size) = (m.position(), m.size());
                (pos.x..pos.x + size.width as i32).contains(&px)
                    && (pos.y..pos.y + size.height as i32).contains(&py)
            })
        });
        let mut attrs = egui_winit::create_winit_window_attributes(&self.ctx, viewport.clone());
        if let Some([x, y, w, h]) = saved {
            // Physical pixels both ways, so a scaled display round-trips exactly.
            // The builder's default size would be re-applied after creation.
            viewport.inner_size = None;
            attrs = attrs
                .with_position(winit::dpi::PhysicalPosition::new(x, y))
                .with_inner_size(winit::dpi::PhysicalSize::new(w.max(1) as u32, h.max(1) as u32));
        }
        let window = el.create_window(attrs).expect("create window");
        egui_winit::apply_viewport_builder_to_window(&self.ctx, &window, &viewport);
        window
    }
}

impl ApplicationHandler<Instant> for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.win.is_some() {
            return;
        }
        let window = Rc::new(self.create_window(el));
        let surface = softbuffer::Surface::new(&self.softbuffer, window.clone()).expect("create surface");
        let state = egui_winit::State::new(
            self.ctx.clone(),
            ViewportId::ROOT,
            el,
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let mut info = egui::ViewportInfo::default();
        egui_winit::update_viewport_info(&mut info, &self.ctx, &window, true);
        window.request_redraw();
        self.win = Some(Win { window, surface, state, info });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.redraw(el),
            WindowEvent::CloseRequested => self.exit(el),
            event => {
                let Some(win) = &mut self.win else { return };
                if win.state.on_window_event(&win.window, &event).repaint {
                    win.window.request_redraw();
                }
            }
        }
    }

    /// A repaint request from egui, possibly from another thread.
    fn user_event(&mut self, _: &ActiveEventLoop, when: Instant) {
        if when <= Instant::now() {
            if let Some(win) = &self.win {
                win.window.request_redraw();
            }
        } else {
            self.repaint_at = Some(self.repaint_at.map_or(when, |at| at.min(when)));
        }
    }

    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            self.repaint_at = None;
            if let Some(win) = &self.win {
                win.window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        el.set_control_flow(self.repaint_at.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

fn main() -> Result<(), winit::error::EventLoopError> {
    // No OS title bar: the app draws its own, including the window buttons.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("rficus")
        .with_inner_size([668.0, 760.0])
        .with_min_inner_size([668.0, 520.0])
        .with_decorations(false);
    viewport.icon = icon().map(std::sync::Arc::new);

    let event_loop = EventLoop::<Instant>::with_user_event().build()?;
    let ctx = egui::Context::default();
    let proxy = event_loop.create_proxy();
    ctx.set_request_repaint_callback(move |info| {
        let _ = proxy.send_event(Instant::now() + info.delay);
    });

    let mut runner = Runner {
        app: ui::App::new(&ctx),
        ctx,
        viewport,
        softbuffer: softbuffer::Context::new(event_loop.owned_display_handle()).expect("softbuffer"),
        // The tile cache costs ~6 MB. Measured on this UI without it: ~2 ms a
        // frame at the default size, and faster than with it at 1080p.
        renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra).with_caching(false),
        win: None,
        repaint_at: None,
    };
    event_loop.run_app(&mut runner)
}
