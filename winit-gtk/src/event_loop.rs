use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gtk::{gdk, gio, glib, prelude::*};
use winit_common::free_unix::is_main_thread;
use winit_core::application::ApplicationHandler;
use winit_core::cursor::{CustomCursor as CoreCustomCursor, CustomCursorSource};
use winit_core::error::{EventLoopError, OsError, RequestError};
use winit_core::event::{DeviceEvent, DeviceId, WindowEvent};
use winit_core::event_loop::pump_events::PumpStatus;
use winit_core::event_loop::{
    ActiveEventLoop as CoreActiveEventLoop, ControlFlow, DeviceEvents,
    EventLoopProxy as CoreEventLoopProxy, EventLoopProxyProvider,
    OwnedDisplayHandle as CoreOwnedDisplayHandle,
};
use winit_core::monitor::MonitorHandle as CoreMonitorHandle;
use winit_core::window::{Theme, Window as CoreWindow, WindowAttributes, WindowId};

use crate::window::{Window, WindowRequest};

#[derive(Debug)]
struct PeekableReceiver<T> {
    recv: crossbeam_channel::Receiver<T>,
    first: Option<T>,
}

impl<T> PeekableReceiver<T> {
    fn new(recv: crossbeam_channel::Receiver<T>) -> Self {
        Self { recv, first: None }
    }

    fn has_incoming(&mut self) -> bool {
        if self.first.is_some() {
            return true;
        }
        match self.recv.try_recv() {
            Ok(v) => {
                self.first = Some(v);
                true
            },
            Err(_) => false,
        }
    }

    fn try_recv(&mut self) -> Result<T, crossbeam_channel::TryRecvError> {
        match self.first.take() {
            Some(v) => Ok(v),
            None => self.recv.try_recv(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum QueuedEvent {
    Window { id: WindowId, event: WindowEvent },
    Device { id: DeviceId, event: DeviceEvent },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlatformSpecificEventLoopAttributes {
    pub any_thread: bool,
    pub app_id: Option<String>,
}

impl Default for PlatformSpecificEventLoopAttributes {
    fn default() -> Self {
        Self { any_thread: false, app_id: None }
    }
}

#[derive(Debug)]
pub struct ActiveEventLoop {
    pub(crate) context: glib::MainContext,
    pub(crate) display: gdk::Display,
    pub(crate) app: gtk::Application,
    pub(crate) windows: Rc<RefCell<HashSet<WindowId>>>,
    pub(crate) window_requests_tx: glib::Sender<(WindowId, WindowRequest)>,
    pub(crate) events_tx: crossbeam_channel::Sender<QueuedEvent>,
    pub(crate) redraw_tx: crossbeam_channel::Sender<WindowId>,

    control_flow: Cell<ControlFlow>,
    exit_code: Cell<Option<i32>>,
    device_events: Cell<DeviceEvents>,
    proxy_wake_flag: Arc<AtomicBool>,
    owned_display: CoreOwnedDisplayHandle,
}

impl ActiveEventLoop {
    pub(crate) fn is_wayland(&self) -> bool {
        self.display.backend().is_wayland()
    }

    pub(crate) fn is_x11(&self) -> bool {
        self.display.backend().is_x11()
    }

    pub(crate) fn gtk_app(&self) -> &gtk::Application {
        &self.app
    }

    pub(crate) fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>) {
        let _ = count;
        let _ = desktop_filename;
        todo!()
    }
}

impl CoreActiveEventLoop for ActiveEventLoop {
    fn create_proxy(&self) -> CoreEventLoopProxy {
        CoreEventLoopProxy::new(Arc::new(EventLoopProxy {
            proxy_wake_flag: self.proxy_wake_flag.clone(),
        }))
    }

    fn create_window(
        &self,
        window_attributes: WindowAttributes,
    ) -> Result<Box<dyn CoreWindow>, RequestError> {
        Ok(Box::new(Window::new(self, window_attributes)?))
    }

    fn create_custom_cursor(
        &self,
        source: CustomCursorSource,
    ) -> Result<CoreCustomCursor, RequestError> {
        let _ = source;
        todo!()
    }

    fn available_monitors(&self) -> Box<dyn Iterator<Item = CoreMonitorHandle>> {
        todo!()
    }

    fn primary_monitor(&self) -> Option<winit_core::monitor::MonitorHandle> {
        todo!()
    }

    fn listen_device_events(&self, allowed: DeviceEvents) {
        self.device_events.set(allowed);
    }

    fn system_theme(&self) -> Option<Theme> {
        todo!()
    }

    fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow);
        self.context.wakeup();
    }

    fn control_flow(&self) -> ControlFlow {
        self.control_flow.get()
    }

    fn exit(&self) {
        self.exit_code.set(Some(0));
        self.context.wakeup();
    }

    fn exiting(&self) -> bool {
        self.exit_code.get().is_some()
    }

    fn owned_display_handle(&self) -> CoreOwnedDisplayHandle {
        self.owned_display.clone()
    }

    fn rwh_06_handle(&self) -> &dyn rwh_06::HasDisplayHandle {
        self
    }
}

impl rwh_06::HasDisplayHandle for ActiveEventLoop {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        self.owned_display.display_handle()
    }
}

#[derive(Debug)]
pub struct EventLoop {
    loop_running: bool,
    context: glib::MainContext,
    window_target: ActiveEventLoop,
    events_rx: PeekableReceiver<QueuedEvent>,
    redraw_rx: PeekableReceiver<WindowId>,
}

impl EventLoop {
    pub fn new(attributes: &PlatformSpecificEventLoopAttributes) -> Result<Self, EventLoopError> {
        static EVENT_LOOP_CREATED: AtomicBool = AtomicBool::new(false);
        if EVENT_LOOP_CREATED.swap(true, Ordering::Relaxed) {
            return Err(EventLoopError::RecreationAttempt);
        }

        if !attributes.any_thread && !is_main_thread() {
            panic!(
                "Initializing the event loop outside of the main thread is a significant \
                cross-platform compatibility hazard. Use `EventLoopBuilderExtGtk::with_any_thread(true)` \
                if you truly need to create an event loop on a different thread."
            );
        }

        let context = glib::MainContext::default();
        context
            .with_thread_default(|| Self::new_gtk(&context, attributes.app_id.as_deref()))
            .map_err(|e| {
                EventLoopError::Os(OsError::new(
                    e.line,
                    e.filename,
                    format!("Failed to initialize GTK thread-default context: {}", e.message),
                ))
            })?
    }

    fn new_gtk(context: &glib::MainContext, app_id: Option<&str>) -> Result<Self, EventLoopError> {
        gtk::init().map_err(|e| EventLoopError::Os(os_error!(e)))?;

        let app = gtk::Application::new(app_id, gio::ApplicationFlags::empty());
        app.register(None::<&gio::Cancellable>).map_err(|e| EventLoopError::Os(os_error!(e)))?;

        let (events_tx, events_rx) = crossbeam_channel::unbounded();
        let (redraw_tx, redraw_rx) = crossbeam_channel::unbounded();

        let (window_requests_tx, window_requests_rx) =
            glib::MainContext::channel(glib::Priority::default());

        let proxy_wake_flag = Arc::new(AtomicBool::new(false));

        let display = gdk::Display::default()
            .ok_or_else(|| EventLoopError::Os(os_error!("GdkDisplay not found")))?;
        let is_wayland = display.backend().is_wayland();

        let owned_handle = Arc::new(OwnedDisplayHandle {
            is_wayland,
            wl_display: None,
            xlib_display: None,
            xlib_screen: 0,
        });
        let owned_display = CoreOwnedDisplayHandle::new(owned_handle);

        let window_target = ActiveEventLoop {
            context: context.clone(),
            display,
            app,
            windows: Default::default(),
            window_requests_tx,
            events_tx,
            redraw_tx,
            control_flow: Default::default(),
            exit_code: Default::default(),
            device_events: Default::default(),
            proxy_wake_flag,
            owned_display,
        };

        window_requests_rx.attach(Some(context), move |(id, request)| {
            let _ = id;
            let _ = request;
            todo!()
        });

        Ok(Self {
            loop_running: false,
            context: context.clone(),
            window_target,
            events_rx: PeekableReceiver::new(events_rx),
            redraw_rx: PeekableReceiver::new(redraw_rx),
        })
    }

    pub fn window_target(&self) -> &dyn CoreActiveEventLoop {
        &self.window_target
    }

    pub fn run_app_on_demand<A: ApplicationHandler>(
        &mut self,
        app: A,
    ) -> Result<(), EventLoopError> {
        let _ = app;
        todo!()
    }

    pub fn pump_app_events<A: ApplicationHandler>(
        &mut self,
        timeout: Option<Duration>,
        app: A,
    ) -> PumpStatus {
        let _ = timeout;
        let _ = app;
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct EventLoopProxy {
    proxy_wake_flag: Arc<AtomicBool>,
}

impl EventLoopProxyProvider for EventLoopProxy {
    fn wake_up(&self) {
        self.proxy_wake_flag.store(true, Ordering::Release);
        glib::MainContext::default().wakeup();
    }
}

#[derive(Debug)]
struct OwnedDisplayHandle {
    is_wayland: bool,
    wl_display: Option<NonNull<std::ffi::c_void>>,
    xlib_display: Option<NonNull<std::ffi::c_void>>,
    xlib_screen: i32,
}

unsafe impl Send for OwnedDisplayHandle {}
unsafe impl Sync for OwnedDisplayHandle {}

impl rwh_06::HasDisplayHandle for OwnedDisplayHandle {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        let raw = if self.is_wayland {
            let wl = self.wl_display.ok_or(rwh_06::HandleError::Unavailable)?;
            let h = rwh_06::WaylandDisplayHandle::new(wl);
            rwh_06::RawDisplayHandle::Wayland(h)
        } else {
            let x = self.xlib_display.ok_or(rwh_06::HandleError::Unavailable)?;
            let h = rwh_06::XlibDisplayHandle::new(Some(x), self.xlib_screen);
            rwh_06::RawDisplayHandle::Xlib(h)
        };

        Ok(unsafe { rwh_06::DisplayHandle::borrow_raw(raw) })
    }
}
