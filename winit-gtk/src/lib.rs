//! # Winit's GTK backend.

#[cfg(all(not(feature = "x11"), not(feature = "wayland")))]
compile_error!("Please select at least one of the following features: `x11`, `wayland`");

use gtk::gdk;
use winit_core::event_loop::ActiveEventLoop as CoreActiveEventLoop;
use winit_core::monitor::MonitorHandle;
use winit_core::window::{PlatformWindowAttributes, Window as CoreWindow};

macro_rules! os_error {
    ($error:expr) => {{ winit_core::error::OsError::new(line!(), file!(), $error) }};
    (glib_bool: $error:expr) => {{ winit_core::error::OsError::new($error.line, $error.filename, $error.message) }};
}

mod event_loop;
mod monitor;
mod window;

use self::event_loop::ActiveEventLoop as GtkActiveEventLoop;
pub use self::event_loop::{EventLoop, PlatformSpecificEventLoopAttributes};
use self::monitor::MonitorHandle as GtkMonitorHandle;
use self::window::Window as GtkWindow;

/// Additional methods on [`Window`] that are specific to GTK.
///
/// [`Window`]: crate::window::Window
pub trait WindowExtGtk {
    fn default_vbox(&self) -> Option<&gtk::Box>;
    fn set_skip_taskbar(&self, skip_taskbar: bool);
    fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>);
}

impl WindowExtGtk for dyn CoreWindow + '_ {
    #[inline]
    fn default_vbox(&self) -> Option<&gtk::Box> {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.default_vbox()
    }

    #[inline]
    fn set_skip_taskbar(&self, skip_taskbar: bool) {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.set_skip_taskbar(skip_taskbar)
    }

    #[inline]
    fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>) {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.set_badge_count(count, desktop_filename)
    }
}

/// Window attributes methods specific to GTK.
#[derive(Debug, Clone)]
pub struct WindowAttributesGtk {
    pub(crate) skip_taskbar: bool,
    pub(crate) auto_transparent: bool,
    pub(crate) double_buffered: bool,
    pub(crate) app_paintable: bool,
    pub(crate) rgba_visual: bool,
    pub(crate) cursor_moved: bool,
    pub(crate) default_vbox: bool,
}

impl WindowAttributesGtk {
    #[inline]
    pub fn with_skip_taskbar(mut self, skip_taskbar: bool) -> Self {
        self.skip_taskbar = skip_taskbar;
        self
    }

    #[inline]
    pub fn with_auto_transparent(mut self, auto_transparent: bool) -> Self {
        self.auto_transparent = auto_transparent;
        self
    }

    #[inline]
    pub fn with_double_buffered(mut self, double_buffered: bool) -> Self {
        self.double_buffered = double_buffered;
        self
    }

    #[inline]
    pub fn with_app_paintable(mut self, app_paintable: bool) -> Self {
        self.app_paintable = app_paintable;
        self
    }

    #[inline]
    pub fn with_rgba_visual(mut self, rgba_visual: bool) -> Self {
        self.rgba_visual = rgba_visual;
        self
    }

    #[inline]
    pub fn with_cursor_moved(mut self, cursor_moved: bool) -> Self {
        self.cursor_moved = cursor_moved;
        self
    }

    #[inline]
    pub fn with_default_vbox(mut self, default_vbox: bool) -> Self {
        self.default_vbox = default_vbox;
        self
    }
}

impl Default for WindowAttributesGtk {
    #[inline]
    fn default() -> Self {
        Self {
            skip_taskbar: false,
            auto_transparent: true,
            double_buffered: true,
            app_paintable: false,
            rgba_visual: false,
            cursor_moved: true,
            default_vbox: true,
        }
    }
}

impl PlatformWindowAttributes for WindowAttributesGtk {
    fn box_clone(&self) -> Box<dyn PlatformWindowAttributes> {
        Box::from(self.clone())
    }
}

/// Additional methods when building event loop that are specific to GTK.
pub trait EventLoopBuilderExtGtk {
    fn with_any_thread(&mut self, any_thread: bool) -> &mut Self;
    fn with_app_id(&mut self, id: impl Into<String>) -> &mut Self;
}

/// Additional methods on [`MonitorHandle`] that are specific to GTK.
pub trait MonitorHandleExtGtk {
    fn gdk_monitor(&self) -> &gdk::Monitor;
}

impl MonitorHandleExtGtk for MonitorHandle {
    fn gdk_monitor(&self) -> &gdk::Monitor {
        let monitor = self.cast_ref::<GtkMonitorHandle>().unwrap();
        &*monitor
    }
}

/// Additional methods on [`ActiveEventLoop`] that are specific to GTK.
pub trait ActiveEventLoopExtGtk {
    fn is_wayland(&self) -> bool;
    fn is_x11(&self) -> bool;
    fn gtk_app(&self) -> &gtk::Application;
    fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>);
}

impl ActiveEventLoopExtGtk for dyn CoreActiveEventLoop + '_ {
    #[inline]
    fn is_wayland(&self) -> bool {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.is_wayland()
    }

    #[inline]
    fn is_x11(&self) -> bool {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.is_x11()
    }

    #[inline]
    fn gtk_app(&self) -> &gtk::Application {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.gtk_app()
    }

    #[inline]
    fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>) {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.set_badge_count(count, desktop_filename)
    }
}
