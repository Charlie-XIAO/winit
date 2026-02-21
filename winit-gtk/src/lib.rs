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
mod window_request;
mod window_state;

use self::event_loop::ActiveEventLoop as GtkActiveEventLoop;
pub use self::event_loop::{EventLoop, PlatformSpecificEventLoopAttributes};
use self::monitor::MonitorHandle as GtkMonitorHandle;
use self::window::Window as GtkWindow;

/// Additional methods on [`Window`] that are specific to GTK.
///
/// [`Window`]: crate::window::Window
pub trait WindowExtGtk {
    fn set_focusable(&self, focusable: bool);

    fn with_gtk_window<F>(&self, f: F)
    where
        F: FnOnce(&gtk::ApplicationWindow) + Send + 'static;

    fn with_gtk_drawing_area<F>(&self, f: F)
    where
        F: FnOnce(&gtk::DrawingArea) + Send + 'static;
}

impl WindowExtGtk for dyn CoreWindow + '_ {
    #[inline]
    fn set_focusable(&self, focusable: bool) {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.set_focusable(focusable)
    }

    #[inline]
    fn with_gtk_window<F>(&self, f: F)
    where
        F: FnOnce(&gtk::ApplicationWindow) + Send + 'static,
    {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.with_gtk_window(f)
    }

    #[inline]
    fn with_gtk_drawing_area<F>(&self, f: F)
    where
        F: FnOnce(&gtk::DrawingArea) + Send + 'static,
    {
        let window = self.cast_ref::<GtkWindow>().unwrap();
        window.with_gtk_drawing_area(f)
    }
}

/// Window attributes methods specific to GTK.
#[derive(Debug, Clone)]
pub struct WindowAttributesGtk {
    pub(crate) focusable: bool,
}

impl WindowAttributesGtk {
    #[inline]
    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }
}

impl Default for WindowAttributesGtk {
    #[inline]
    fn default() -> Self {
        Self { focusable: true }
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
        &monitor.0
    }
}

/// Additional methods on [`ActiveEventLoop`] that are specific to GTK.
pub trait ActiveEventLoopExtGtk {
    fn is_wayland(&self) -> bool;
    fn is_x11(&self) -> bool;
    fn gtk_app(&self) -> &gtk::Application;
}

impl ActiveEventLoopExtGtk for dyn CoreActiveEventLoop + '_ {
    #[inline]
    fn is_wayland(&self) -> bool {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.backend().is_wayland()
    }

    #[inline]
    fn is_x11(&self) -> bool {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.backend().is_x11()
    }

    #[inline]
    fn gtk_app(&self) -> &gtk::Application {
        let event_loop = self.cast_ref::<GtkActiveEventLoop>().unwrap();
        event_loop.gtk_app()
    }
}
