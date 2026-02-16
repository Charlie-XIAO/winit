use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use dpi::{PhysicalInsets, PhysicalPosition, PhysicalSize, Position, Size};
use gtk::{gdk, gdk_pixbuf, glib, prelude::*};
use winit_core::cursor::Cursor;
use winit_core::error::{NotSupportedError, RequestError};
use winit_core::icon::{Icon, RgbaIcon};
use winit_core::monitor::{Fullscreen, MonitorHandle as CoreMonitorHandle};
use winit_core::window::{
    CursorGrabMode, ImeCapabilities, ImeRequest, ImeRequestError, ResizeDirection, Theme,
    UserAttentionType, Window as CoreWindow, WindowAttributes, WindowButtons, WindowId,
    WindowLevel,
};

use crate::WindowAttributesGtk;
use crate::event_loop::{ActiveEventLoop, EventLoopWindow, OwnedDisplayHandle};
use crate::monitor::MonitorHandle;
use crate::window_request::WindowRequest;

const GTK_DARK_THEME_SUFFIXES: &[&str] = &["-dark", "-Dark", "-Darker"];

#[derive(Debug)]
struct WindowState {
    scale_factor: AtomicI32,
}

#[derive(Debug)]
pub struct Window {
    id: WindowId,
    raw: OwnedWindowHandle,
    state: Arc<WindowState>,
    context: glib::MainContext,
    handle: Arc<OwnedDisplayHandle>,
    redraw_tx: crossbeam_channel::Sender<WindowId>,
    window_requests_tx: async_channel::Sender<(WindowId, WindowRequest)>,
}

impl Window {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        mut attributes: WindowAttributes,
    ) -> Result<Self, RequestError> {
        let pl_attributes = attributes
            .platform
            .take()
            .and_then(|attrs| attrs.cast::<WindowAttributesGtk>().ok())
            .unwrap_or_default();

        let mut builder = gtk::ApplicationWindow::builder()
            .application(&event_loop.app)
            .deletable(attributes.enabled_buttons.contains(WindowButtons::CLOSE))
            .title(attributes.title)
            .visible(attributes.visible)
            .decorated(attributes.decorations)
            .accept_focus(attributes.active && pl_attributes.focusable)
            .skip_taskbar_hint(pl_attributes.skip_taskbar)
            .skip_pager_hint(pl_attributes.skip_taskbar);

        if let Some(window_icon) = attributes.window_icon
            && let Some(icon) = window_icon.cast_ref::<RgbaIcon>()
        {
            let pixbuf = pixbuf_from_rgba_icon(icon);
            builder = builder.icon(&pixbuf);
        }

        let window = builder.build();

        let scale_factor = window.scale_factor();

        let (width, height) = attributes
            .surface_size
            .map(|size| size.to_logical::<i32>(scale_factor as _).into())
            .unwrap_or((800, 600));
        window.set_default_size(1, 1);
        window.resize(width, height);

        let mut geometry_mask = gdk::WindowHints::empty();
        let (min_width, min_height) = attributes
            .min_surface_size
            .inspect(|_| geometry_mask |= gdk::WindowHints::MIN_SIZE)
            .map(|size| size.to_logical::<i32>(scale_factor as _).into())
            .unwrap_or((-1, -1));
        let (max_width, max_height) = attributes
            .max_surface_size
            .inspect(|_| geometry_mask |= gdk::WindowHints::MAX_SIZE)
            .map(|size| size.to_logical::<i32>(scale_factor as _).into())
            .unwrap_or((-1, -1));
        let (width_inc, height_inc) = attributes
            .surface_resize_increments
            .inspect(|_| geometry_mask |= gdk::WindowHints::RESIZE_INC)
            .map(|size| size.to_logical::<i32>(scale_factor as _).into())
            .unwrap_or((0, 0));
        window.set_geometry_hints(
            None::<&gtk::Window>,
            Some(&gdk::Geometry::new(
                min_width,
                min_height,
                max_width,
                max_height,
                0,
                0,
                width_inc,
                height_inc,
                0.,
                0.,
                gdk::Gravity::NorthWest,
            )),
            geometry_mask,
        );

        if let Some(position) = attributes.position {
            let (x, y) = position.to_logical::<i32>(scale_factor as _).into();
            window.move_(x, y);
        }

        if attributes.maximized {
            struct Process {
                window: gtk::ApplicationWindow,
                resizable: bool,
                step: u8,
            }

            let process = Rc::new(RefCell::new(Process {
                window: window.clone(),
                resizable: attributes.resizable,
                step: 0,
            }));

            // We cannot maximize a non-resizable window so we have to do it in
            // steps and finally restore the resizable state
            glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
                let mut process = process.borrow_mut();
                match process.step {
                    0 => {
                        process.window.set_resizable(true);
                        process.step = 1;
                        glib::ControlFlow::Continue
                    },
                    1 => {
                        process.window.maximize();
                        process.step = 2;
                        glib::ControlFlow::Continue
                    },
                    _ => {
                        process.window.set_resizable(process.resizable);
                        glib::ControlFlow::Break
                    },
                }
            });
        } else {
            window.set_resizable(attributes.resizable);
        }

        match attributes.window_level {
            WindowLevel::Normal => {},
            WindowLevel::AlwaysOnTop => window.set_keep_above(true),
            WindowLevel::AlwaysOnBottom => window.set_keep_below(true),
        }

        if let Some(settings) = gtk::Settings::default()
            && let Some(preferred_theme) = attributes.preferred_theme
        {
            match preferred_theme {
                Theme::Dark => {
                    settings.set_gtk_application_prefer_dark_theme(true);
                },
                Theme::Light => {
                    settings.set_gtk_application_prefer_dark_theme(false);
                    // If current theme name ends with a dark suffix, just
                    // setting prefer-dark-theme to false won't be enough, and
                    // we also have to remove the dark suffix
                    if let Some(name) = settings.gtk_theme_name()
                        && let Some(base_name) = GTK_DARK_THEME_SUFFIXES
                            .iter()
                            .find(|s| name.ends_with(*s))
                            .map(|s| name.strip_suffix(s))
                    {
                        settings.set_gtk_theme_name(base_name);
                    }
                },
            }
        }

        if let Some(ref fullscreen) = attributes.fullscreen {
            match fullscreen {
                Fullscreen::Borderless(Some(m)) => {
                    let display = window.display();
                    if let Some(target) = m.cast_ref::<MonitorHandle>() {
                        for i in 0..display.n_monitors() {
                            if let Some(monitor) = display.monitor(i)
                                && monitor == target.0
                            {
                                let screen = display.default_screen();
                                window.fullscreen_on_monitor(&screen, i);
                                break;
                            }
                        }
                    }
                },
                Fullscreen::Borderless(None) => {
                    window.fullscreen();
                },
                Fullscreen::Exclusive(_, _) => {
                    return Err(RequestError::NotSupported(NotSupportedError::new(
                        "GTK backend does not support exclusive fullscreen modes",
                    )));
                },
            }
        }

        if pl_attributes.app_paintable || attributes.transparent {
            window.set_app_paintable(true);
        }

        if (pl_attributes.rgba_visual || attributes.transparent)
            && let Some(screen) = gtk::prelude::GtkWindowExt::screen(&window)
            && let Some(visual) = screen.rgba_visual()
        {
            window.set_visual(Some(&visual));
        }

        let default_vbox = if pl_attributes.default_vbox {
            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
            window.add(&vbox);
            Some(vbox)
        } else {
            None
        };

        // TODO: handle attributes.cursor

        if attributes.visible {
            window.show_all();
            if attributes.active {
                window.present();
            }
        } else {
            window.hide();
        }

        // If the window was created as not active (focused) but needs to be
        // focusable, we should restore accept-focus after the first draw
        if pl_attributes.focusable && !attributes.active {
            let signal_id = Rc::new(RefCell::new(None));
            let id = {
                let signal_id = signal_id.clone();
                window.connect_draw(move |w, _| {
                    if let Some(id) = signal_id.borrow_mut().take() {
                        w.set_accept_focus(true);
                        w.disconnect(id);
                    }
                    glib::Propagation::Proceed
                })
            };
            *signal_id.borrow_mut() = Some(id);
        }

        let state = Arc::new(WindowState { scale_factor: AtomicI32::new(scale_factor) });

        {
            let state = state.clone();
            window.connect_scale_factor_notify(move |w| {
                state.scale_factor.store(w.scale_factor(), Ordering::Release);
            });
        }

        let id = WindowId::from_raw(window.id() as _);
        event_loop
            .windows
            .borrow_mut()
            .insert(id, EventLoopWindow { window: window.clone(), default_vbox });

        if let Err(e) = event_loop.window_requests_tx.send_blocking((
            id,
            WindowRequest::WireUpEvents {
                transparent_draw: attributes.transparent && pl_attributes.transparent_draw,
                pointer_moved: pl_attributes.pointer_moved,
                fullscreen: attributes.fullscreen.is_some(),
            },
        )) {
            tracing::warn!("Failed to send WindowRequest::WireUpEvents: {e}");
        }
        event_loop.context.wakeup();

        window.realize(); // Ensure window.window() is created
        let raw = window.window().map_or(OwnedWindowHandle::Unavailable, |window| {
            OwnedWindowHandle::new(&window, event_loop.backend())
        });

        Ok(Self {
            id,
            raw,
            state,
            context: event_loop.context.clone(),
            handle: event_loop.handle.clone(),
            redraw_tx: event_loop.redraw_tx.clone(),
            window_requests_tx: event_loop.window_requests_tx.clone(),
        })
    }

    pub(crate) fn set_focusable(&self, focusable: bool) {
        let _ = focusable;
        todo!()
    }

    pub(crate) fn set_skip_taskbar(&self, skip_taskbar: bool) {
        let _ = skip_taskbar;
        todo!()
    }

    pub(crate) fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>) {
        let _ = count;
        let _ = desktop_filename;
        todo!()
    }

    pub(crate) fn with_gtk_window<F>(&self, f: F)
    where
        F: FnOnce(&gtk::ApplicationWindow) + Send + 'static,
    {
        if let Err(e) = self
            .window_requests_tx
            .send_blocking((self.id, WindowRequest::WithGtkWindow(Box::new(f))))
        {
            tracing::warn!("Failed to send WindowRequest::WithGtkWindow: {e}");
        }
        self.context.wakeup();
    }

    pub(crate) fn with_default_vbox<F>(&self, f: F)
    where
        F: FnOnce(Option<&gtk::Box>) + Send + 'static,
    {
        if let Err(e) = self
            .window_requests_tx
            .send_blocking((self.id, WindowRequest::WithDefaultVbox(Box::new(f))))
        {
            tracing::warn!("Failed to send WindowRequest::WithDefaultVbox: {e}");
        }
        self.context.wakeup();
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        if let Err(e) = self.window_requests_tx.send_blocking((self.id, WindowRequest::Destroy)) {
            tracing::warn!("Failed to send WindowRequest::Destroy: {e}");
        }
        self.context.wakeup();
    }
}

impl rwh_06::HasWindowHandle for Window {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        self.raw.window_handle()
    }
}

impl rwh_06::HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        self.handle.display_handle()
    }
}

impl CoreWindow for Window {
    fn id(&self) -> WindowId {
        self.id
    }

    fn scale_factor(&self) -> f64 {
        self.state.scale_factor.load(Ordering::Acquire) as _
    }

    fn request_redraw(&self) {
        let _ = self.redraw_tx.send(self.id);
        self.context.wakeup();
    }

    fn pre_present_notify(&self) {}

    fn reset_dead_keys(&self) {
        todo!()
    }

    fn surface_position(&self) -> PhysicalPosition<i32> {
        todo!()
    }

    fn outer_position(&self) -> Result<PhysicalPosition<i32>, RequestError> {
        todo!()
    }

    fn set_outer_position(&self, position: Position) {
        let _ = position;
        todo!()
    }

    fn surface_size(&self) -> PhysicalSize<u32> {
        todo!()
    }

    fn request_surface_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let _ = size;
        todo!()
    }

    fn outer_size(&self) -> PhysicalSize<u32> {
        todo!()
    }

    fn safe_area(&self) -> PhysicalInsets<u32> {
        todo!()
    }

    fn set_min_surface_size(&self, min_size: Option<Size>) {
        let _ = min_size;
        todo!()
    }

    fn set_max_surface_size(&self, max_size: Option<Size>) {
        let _ = max_size;
        todo!()
    }

    fn surface_resize_increments(&self) -> Option<PhysicalSize<u32>> {
        todo!()
    }

    fn set_surface_resize_increments(&self, increments: Option<Size>) {
        let _ = increments;
        todo!()
    }

    fn set_title(&self, title: &str) {
        if let Err(e) = self
            .window_requests_tx
            .send_blocking((self.id, WindowRequest::Title(title.to_string())))
        {
            tracing::warn!("Failed to send WindowRequest::Title: {e}");
        }
        self.context.wakeup();
    }

    fn set_transparent(&self, transparent: bool) {
        let _ = transparent;
        todo!()
    }

    fn set_blur(&self, blur: bool) {
        let _ = blur;
        todo!()
    }

    fn set_visible(&self, visible: bool) {
        if let Err(e) =
            self.window_requests_tx.send_blocking((self.id, WindowRequest::Visible(visible)))
        {
            tracing::warn!("Failed to send WindowRequest::Visible: {e}");
        }
        self.context.wakeup();
    }

    fn is_visible(&self) -> Option<bool> {
        todo!()
    }

    fn set_resizable(&self, resizable: bool) {
        if let Err(e) =
            self.window_requests_tx.send_blocking((self.id, WindowRequest::Resizable(resizable)))
        {
            tracing::warn!("Failed to send WindowRequest::Resizable: {e}");
        }
        self.context.wakeup();
    }

    fn is_resizable(&self) -> bool {
        todo!()
    }

    fn set_enabled_buttons(&self, buttons: WindowButtons) {
        let _ = buttons;
        todo!()
    }

    fn enabled_buttons(&self) -> WindowButtons {
        todo!()
    }

    fn set_minimized(&self, minimized: bool) {
        let _ = minimized;
        todo!()
    }

    fn is_minimized(&self) -> Option<bool> {
        todo!()
    }

    fn set_maximized(&self, maximized: bool) {
        let _ = maximized;
        todo!()
    }

    fn is_maximized(&self) -> bool {
        todo!()
    }

    fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        let _ = fullscreen;
        todo!()
    }

    fn fullscreen(&self) -> Option<Fullscreen> {
        todo!()
    }

    fn set_decorations(&self, decorations: bool) {
        let _ = decorations;
        todo!()
    }

    fn is_decorated(&self) -> bool {
        todo!()
    }

    fn set_window_level(&self, level: WindowLevel) {
        let _ = level;
        todo!()
    }

    fn set_window_icon(&self, window_icon: Option<Icon>) {
        let _ = window_icon;
        todo!()
    }

    fn request_ime_update(&self, request: ImeRequest) -> Result<(), ImeRequestError> {
        let _ = request;
        todo!()
    }

    fn ime_capabilities(&self) -> Option<ImeCapabilities> {
        todo!()
    }

    fn focus_window(&self) {
        todo!()
    }

    fn has_focus(&self) -> bool {
        todo!()
    }

    fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        let _ = request_type;
        todo!()
    }

    fn set_theme(&self, theme: Option<Theme>) {
        let _ = theme;
        todo!()
    }

    fn theme(&self) -> Option<Theme> {
        todo!()
    }

    fn set_content_protected(&self, protected: bool) {
        let _ = protected;
        todo!();
    }

    fn title(&self) -> String {
        todo!()
    }

    fn set_cursor(&self, cursor: Cursor) {
        let _ = cursor;
        todo!()
    }

    fn set_cursor_position(&self, position: Position) -> Result<(), RequestError> {
        let _ = position;
        todo!();
    }

    fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), RequestError> {
        let _ = mode;
        todo!();
    }

    fn set_cursor_visible(&self, visible: bool) {
        let _ = visible;
        todo!();
    }

    fn drag_window(&self) -> Result<(), RequestError> {
        todo!()
    }

    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), RequestError> {
        let _ = direction;
        todo!();
    }

    fn show_window_menu(&self, position: Position) {
        let _ = position;
        todo!()
    }

    fn set_cursor_hittest(&self, hittest: bool) -> Result<(), RequestError> {
        let _ = hittest;
        todo!()
    }

    fn current_monitor(&self) -> Option<CoreMonitorHandle> {
        todo!()
    }

    fn available_monitors(&self) -> Box<dyn Iterator<Item = CoreMonitorHandle>> {
        todo!()
    }

    fn primary_monitor(&self) -> Option<CoreMonitorHandle> {
        todo!()
    }

    fn rwh_06_display_handle(&self) -> &dyn rwh_06::HasDisplayHandle {
        self
    }

    fn rwh_06_window_handle(&self) -> &dyn rwh_06::HasWindowHandle {
        self
    }
}

#[derive(Debug)]
enum OwnedWindowHandle {
    Wayland { surface: NonNull<c_void> },
    X11 { xid: u64 },
    Unavailable,
}

unsafe impl Send for OwnedWindowHandle {}
unsafe impl Sync for OwnedWindowHandle {}

impl OwnedWindowHandle {
    fn new(window: &gdk::Window, backend: gdk::Backend) -> Self {
        if backend.is_wayland() {
            #[cfg(feature = "wayland")]
            {
                let wl = unsafe {
                    gdk_wayland_sys::gdk_wayland_window_get_wl_surface(window.as_ptr() as *mut _)
                };
                match NonNull::new(wl) {
                    Some(surface) => Self::Wayland { surface },
                    None => Self::Unavailable,
                }
            }
            #[cfg(not(feature = "wayland"))]
            panic!("GDK backend is wayland but winit-gtk was built without the `wayland` feature");
        } else if backend.is_x11() {
            #[cfg(feature = "x11")]
            {
                let xid = unsafe { gdk_x11_sys::gdk_x11_window_get_xid(window.as_ptr() as *mut _) };
                if xid == 0 { Self::Unavailable } else { Self::X11 { xid } }
            }
            #[cfg(not(feature = "x11"))]
            panic!("GDK backend is X11 but winit-gtk was built without the `x11` feature");
        } else {
            Self::Unavailable
        }
    }
}

impl rwh_06::HasWindowHandle for OwnedWindowHandle {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        let raw = match *self {
            Self::Wayland { surface } => {
                let h = rwh_06::WaylandWindowHandle::new(surface);
                rwh_06::RawWindowHandle::Wayland(h)
            },
            Self::X11 { xid } => {
                let h = rwh_06::XlibWindowHandle::new(xid);
                rwh_06::RawWindowHandle::Xlib(h)
            },
            Self::Unavailable => return Err(rwh_06::HandleError::Unavailable),
        };

        Ok(unsafe { rwh_06::WindowHandle::borrow_raw(raw) })
    }
}

fn pixbuf_from_rgba_icon(icon: &RgbaIcon) -> gdk_pixbuf::Pixbuf {
    let width = icon.width() as i32;
    let height = icon.height() as i32;

    let rowstride = gdk_pixbuf::Pixbuf::calculate_rowstride(
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width,
        height,
    );

    gdk_pixbuf::Pixbuf::from_mut_slice(
        icon.buffer().to_vec(),
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width,
        height,
        rowstride,
    )
}
