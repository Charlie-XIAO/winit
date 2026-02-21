use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use dpi::{PhysicalInsets, PhysicalPosition, PhysicalSize, Position, Size};
use gtk::prelude::*;
use gtk::{gdk, glib};
use winit_core::cursor::Cursor;
use winit_core::error::{NotSupportedError, RequestError};
use winit_core::icon::Icon;
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
use crate::window_state::SharedWindowState;

const GTK_DARK_THEME_SUFFIXES: &[&str] = &["-dark", "-Dark", "-Darker"];

#[derive(Debug)]
pub struct Window {
    id: WindowId,
    raw: OwnedWindowHandle,
    state: SharedWindowState,
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

        let window = gtk::ApplicationWindow::builder()
            .application(&event_loop.app)
            .deletable(attributes.enabled_buttons.contains(WindowButtons::CLOSE))
            .title(attributes.title)
            .visible(attributes.visible)
            .decorated(attributes.decorations)
            .can_focus(attributes.active && pl_attributes.focusable)
            .build();

        let scale_factor = window.scale_factor();

        let (width, height) = attributes
            .surface_size
            .map_or((800, 600), |size| size.to_logical::<i32>(scale_factor as _).into());
        window.set_default_size(width, height);

        if let Some(min_surface_size) = attributes.min_surface_size {
            let (min_width, min_height) =
                min_surface_size.to_logical::<i32>(scale_factor as _).into();
            window.set_size_request(min_width, min_height);
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

        // TODO: handle attributes.icon
        // TODO: handle attributes.position
        // TODO: handle attributes.window_level

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
                    let display = gtk::prelude::RootExt::display(&window);
                    if let Some(target) = m.cast_ref::<MonitorHandle>() {
                        let found = display
                            .monitors()
                            .iter::<gdk::Monitor>()
                            .any(|res| res.ok().map_or(false, |m| m == target.0));
                        if found {
                            window.fullscreen_on_monitor(&target.0);
                        } else {
                            tracing::warn!("Cannot find the monitor specified for fullscreen");
                        }
                    }
                },
                Fullscreen::Borderless(None) => {
                    window.fullscreen();
                },
                Fullscreen::Exclusive(..) => {
                    tracing::warn!("GTK backend does not support exclusive fullscreen mode");
                },
            }
        }

        let drawing_area = gtk::DrawingArea::new();
        drawing_area.set_hexpand(true);
        drawing_area.set_vexpand(true);
        window.set_child(Some(&drawing_area));

        // TODO: handle attributes.cursor

        if attributes.visible {
            window.present();
        }

        let id = WindowId::from_raw(window.id() as _);
        let state = SharedWindowState::new(&window, &drawing_area);
        event_loop.windows.borrow_mut().insert(id, EventLoopWindow {
            window: window.clone(),
            drawing_area,
            state: state.clone(),
        });

        if let Err(e) =
            event_loop.window_requests_tx.send_blocking((id, WindowRequest::WireUpEvents {
                fullscreen: attributes.fullscreen.is_some(),
            }))
        {
            tracing::warn!("Failed to send WindowRequest::WireUpEvents: {e}");
        }
        event_loop.context.wakeup();

        let raw = window.surface().map_or(OwnedWindowHandle::Unavailable, |surface| {
            OwnedWindowHandle::new(&surface, event_loop.backend())
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

    pub(crate) fn with_gtk_drawing_area<F>(&self, f: F)
    where
        F: FnOnce(&gtk::DrawingArea) + Send + 'static,
    {
        if let Err(e) = self
            .window_requests_tx
            .send_blocking((self.id, WindowRequest::WithGtkDrawingArea(Box::new(f))))
        {
            tracing::warn!("Failed to send WindowRequest::WithGtkDrawingArea: {e}");
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
        self.state.scale_factor()
    }

    fn request_redraw(&self) {
        let _ = self.redraw_tx.send(self.id);
        self.context.wakeup();
    }

    fn pre_present_notify(&self) {
        // TODO: should this do anything?
    }

    fn reset_dead_keys(&self) {
        todo!()
    }

    fn surface_position(&self) -> PhysicalPosition<i32> {
        (0, 0).into()
    }

    fn outer_position(&self) -> Result<PhysicalPosition<i32>, RequestError> {
        Err(RequestError::NotSupported(NotSupportedError::new(
            "window position information is not available on GTK",
        )))
    }

    fn set_outer_position(&self, _position: Position) {
        // Not possible
    }

    fn surface_size(&self) -> PhysicalSize<u32> {
        self.state.surface_size()
    }

    fn request_surface_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        let _ = size;
        todo!()
    }

    fn outer_size(&self) -> PhysicalSize<u32> {
        self.state.outer_size()
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
        None // Not supported by GTK
    }

    fn set_surface_resize_increments(&self, increments: Option<Size>) {
        tracing::warn!("GTK does not support setting surface resize increments");
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
    fn new(surface: &gdk::Surface, backend: gdk::Backend) -> Self {
        if backend.is_wayland() {
            #[cfg(feature = "wayland")]
            {
                let wl = unsafe {
                    gdk_wayland_sys::gdk_wayland_surface_get_wl_surface(surface.as_ptr() as *mut _)
                };
                NonNull::new(wl).map_or(Self::Unavailable, |surface| Self::Wayland { surface })
            }
            #[cfg(not(feature = "wayland"))]
            panic!("GDK backend is wayland but winit-gtk was built without the `wayland` feature");
        } else if backend.is_x11() {
            #[cfg(feature = "x11")]
            {
                let xid =
                    unsafe { gdk_x11_sys::gdk_x11_surface_get_xid(surface.as_ptr() as *mut _) };
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
