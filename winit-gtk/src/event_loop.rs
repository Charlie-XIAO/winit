use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use winit_common::free_unix::is_main_thread;
use winit_core::application::ApplicationHandler;
use winit_core::cursor::{CustomCursor as CoreCustomCursor, CustomCursorSource};
use winit_core::error::{EventLoopError, RequestError};
use winit_core::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit_core::event_loop::pump_events::PumpStatus;
use winit_core::event_loop::{
    ActiveEventLoop as CoreActiveEventLoop, ControlFlow, DeviceEvents,
    EventLoopProxy as CoreEventLoopProxy, EventLoopProxyProvider,
    OwnedDisplayHandle as CoreOwnedDisplayHandle,
};
use winit_core::monitor::MonitorHandle as CoreMonitorHandle;
use winit_core::window::{Theme, Window as CoreWindow, WindowAttributes, WindowId};

use crate::monitor::MonitorHandle;
use crate::window::Window;
use crate::window_request::{WindowRequest, handle_window_requests};
use crate::window_state::SharedWindowState;

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

#[derive(Debug, Clone)]
pub(crate) struct EventLoopWindow {
    pub(crate) window: gtk::ApplicationWindow,
    pub(crate) default_vbox: Option<gtk::Box>,
    pub(crate) state: SharedWindowState,
}

pub(crate) type EventLoopWindows = Rc<RefCell<HashMap<WindowId, EventLoopWindow>>>;

#[derive(Debug)]
pub struct ActiveEventLoop {
    pub(crate) context: glib::MainContext,
    pub(crate) display: gdk::Display,
    pub(crate) app: gtk::Application,
    pub(crate) windows: EventLoopWindows,
    pub(crate) window_requests_tx: async_channel::Sender<(WindowId, WindowRequest)>,
    pub(crate) events_tx: crossbeam_channel::Sender<QueuedEvent>,
    pub(crate) redraw_tx: crossbeam_channel::Sender<WindowId>,
    pub(crate) handle: Arc<OwnedDisplayHandle>,

    control_flow: Cell<ControlFlow>,
    exit_code: Cell<Option<i32>>,
    device_events: Cell<DeviceEvents>,
    proxy_wake_flag: Arc<AtomicBool>,
}

impl ActiveEventLoop {
    fn clear_exit(&self) {
        self.exit_code.set(None);
    }

    pub(crate) fn backend(&self) -> gdk::Backend {
        self.display.backend()
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
        let n = self.display.n_monitors();
        let monitors: Vec<_> = (0..n)
            .filter_map(|i| self.display.monitor(i))
            .map(|m| CoreMonitorHandle(Arc::new(MonitorHandle(m))))
            .collect();
        Box::new(monitors.into_iter())
    }

    fn primary_monitor(&self) -> Option<CoreMonitorHandle> {
        self.display.primary_monitor().map(|m| CoreMonitorHandle(Arc::new(MonitorHandle(m))))
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
        CoreOwnedDisplayHandle::new(self.handle.clone())
    }

    fn rwh_06_handle(&self) -> &dyn rwh_06::HasDisplayHandle {
        self
    }
}

impl rwh_06::HasDisplayHandle for ActiveEventLoop {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        self.handle.display_handle()
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
                 cross-platform compatibility hazard. Use \
                 `EventLoopBuilderExtGtk::with_any_thread(true)` if you truly need to create an \
                 event loop on a different thread."
            );
        }

        let context = glib::MainContext::default();
        context
            .with_thread_default(|| Self::new_gtk(&context, attributes.app_id.as_deref()))
            .map_err(|e| EventLoopError::Os(os_error!(glib_bool: e)))?
    }

    fn new_gtk(context: &glib::MainContext, app_id: Option<&str>) -> Result<Self, EventLoopError> {
        gtk::init().map_err(|e| EventLoopError::Os(os_error!(e)))?;

        let app = gtk::Application::new(app_id, gio::ApplicationFlags::empty());
        app.register(None::<&gio::Cancellable>).map_err(|e| EventLoopError::Os(os_error!(e)))?;

        let (events_tx, events_rx) = crossbeam_channel::unbounded();
        let (redraw_tx, redraw_rx) = crossbeam_channel::unbounded();
        let (window_requests_tx, window_requests_rx) = async_channel::unbounded();

        let proxy_wake_flag = Arc::new(AtomicBool::new(false));

        let display = gdk::Display::default()
            .ok_or_else(|| EventLoopError::Os(os_error!("GdkDisplay not found")))?;
        let handle = OwnedDisplayHandle::new(&display);

        let window_target = ActiveEventLoop {
            context: context.clone(),
            display,
            app: app.clone(),
            windows: Default::default(),
            window_requests_tx,
            events_tx: events_tx.clone(),
            redraw_tx: redraw_tx.clone(),
            handle: Arc::new(handle),
            control_flow: Default::default(),
            exit_code: Default::default(),
            device_events: Default::default(),
            proxy_wake_flag,
        };

        context.spawn_local(handle_window_requests(
            window_target.windows.clone(),
            window_requests_rx,
            events_tx,
            redraw_tx,
        ));

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
        mut app: A,
    ) -> Result<(), EventLoopError> {
        self.window_target.clear_exit();

        self.context
            .clone()
            .with_thread_default(|| {
                loop {
                    match self.pump_app_events(None, &mut app) {
                        PumpStatus::Exit(0) => break Ok(()),
                        PumpStatus::Exit(code) => break Err(EventLoopError::ExitFailure(code)),
                        PumpStatus::Continue => continue,
                    }
                }
            })
            .map_err(|e| EventLoopError::Os(os_error!(glib_bool: e)))?
    }

    pub fn pump_app_events<A: ApplicationHandler>(
        &mut self,
        timeout: Option<Duration>,
        mut app: A,
    ) -> PumpStatus {
        self.context
            .clone()
            .with_thread_default(|| {
                if !self.loop_running {
                    self.loop_running = true;
                    self.window_target.app.activate();
                    self.single_iteration(&mut app, StartCause::Init);
                }

                if !self.window_target.exiting() {
                    self.poll_events_with_timeout(timeout, &mut app);
                }

                if let Some(code) = self.window_target.exit_code.get() {
                    self.loop_running = false;
                    PumpStatus::Exit(code)
                } else {
                    PumpStatus::Continue
                }
            })
            .unwrap_or_else(|_| PumpStatus::Exit(1))
    }

    fn has_pending(&mut self) -> bool {
        self.events_rx.has_incoming()
            || self.redraw_rx.has_incoming()
            || self.window_target.proxy_wake_flag.load(Ordering::Acquire)
            || gtk::events_pending()
    }

    fn poll_events_with_timeout<A: ApplicationHandler>(
        &mut self,
        mut timeout: Option<Duration>,
        app: &mut A,
    ) {
        let start = Instant::now();

        let has_pending = self.has_pending();
        if has_pending {
            timeout = Some(Duration::ZERO);
        } else {
            let control_flow_timeout = match self.window_target.control_flow() {
                ControlFlow::Wait => None,
                ControlFlow::Poll => Some(Duration::ZERO),
                ControlFlow::WaitUntil(deadline) => Some(deadline.saturating_duration_since(start)),
            };
            timeout = match (timeout, control_flow_timeout) {
                (None, x) | (x, None) => x,
                (Some(left), Some(right)) => Some(left.min(right)),
            };
        }

        self.gtk_poll(timeout);

        let cause = match self.window_target.control_flow() {
            ControlFlow::Poll => StartCause::Poll,
            ControlFlow::Wait => StartCause::WaitCancelled { start, requested_resume: None },
            ControlFlow::WaitUntil(deadline) => {
                if Instant::now() < deadline {
                    StartCause::WaitCancelled { start, requested_resume: Some(deadline) }
                } else {
                    StartCause::ResumeTimeReached { start, requested_resume: deadline }
                }
            },
        };

        if !self.has_pending()
            && !matches!(cause, StartCause::ResumeTimeReached { .. } | StartCause::Poll)
            && timeout.is_none()
        {
            return;
        }

        self.single_iteration(app, cause)
    }

    fn gtk_poll(&self, timeout: Option<Duration>) {
        match timeout {
            Some(timeout) if timeout == Duration::ZERO => {
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
            },
            Some(timeout) => {
                let timer_fired = Rc::new(Cell::new(false));

                let timer_fired_cloned = timer_fired.clone();
                let timer_source_id = glib::timeout_add_local(timeout, move || {
                    timer_fired_cloned.set(true);
                    glib::ControlFlow::Break
                });

                gtk::main_iteration_do(true);
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }

                // The flag was not set meaning something else woke us first, so
                // we remove the timer to avoid later spurious wakeups
                if !timer_fired.get() {
                    timer_source_id.remove();
                }
            },
            None => {
                gtk::main_iteration_do(true);
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
            },
        }
    }

    fn single_iteration<A: ApplicationHandler>(&mut self, app: &mut A, cause: StartCause) {
        app.new_events(&self.window_target, cause);

        if cause == StartCause::Init {
            app.can_create_surfaces(&self.window_target);
        }

        self.drain_events(app);

        if self.window_target.proxy_wake_flag.swap(false, Ordering::AcqRel) {
            app.proxy_wake_up(&self.window_target);
        }

        {
            let mut redraws = HashSet::new();
            while let Ok(id) = self.redraw_rx.try_recv() {
                redraws.insert(id);
            }
            for id in redraws {
                app.window_event(&self.window_target, id, WindowEvent::RedrawRequested);
            }
        }

        app.about_to_wait(&self.window_target);
    }

    fn drain_events<A: ApplicationHandler>(&mut self, app: &mut A) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                QueuedEvent::Window { id, event } => {
                    app.window_event(&self.window_target, id, event)
                },
                QueuedEvent::Device { id, event } => {
                    app.device_event(&self.window_target, Some(id), event);
                },
            }
        }
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
pub(crate) enum OwnedDisplayHandle {
    Wayland { display: NonNull<c_void> },
    X11 { display: NonNull<c_void>, screen: i32 },
    Unavailable,
}

unsafe impl Send for OwnedDisplayHandle {}
unsafe impl Sync for OwnedDisplayHandle {}

impl OwnedDisplayHandle {
    fn new(display: &gdk::Display) -> Self {
        let backend = display.backend();

        if backend.is_wayland() {
            #[cfg(feature = "wayland")]
            {
                let wl = unsafe {
                    gdk_wayland_sys::gdk_wayland_display_get_wl_display(display.as_ptr() as *mut _)
                };
                NonNull::new(wl).map_or(Self::Unavailable, |display| Self::Wayland { display })
            }
            #[cfg(not(feature = "wayland"))]
            panic!("GDK backend is wayland but winit-gtk was built without the `wayland` feature");
        } else if backend.is_x11() {
            #[cfg(feature = "x11")]
            if let Ok(xlib) = x11_dl::xlib::Xlib::open() {
                let dpy = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
                let screen = (!dpy.is_null())
                    .then(|| unsafe { (xlib.XDefaultScreen)(dpy) })
                    .unwrap_or_default();
                NonNull::new(dpy as *mut _)
                    .map_or(Self::Unavailable, |display| Self::X11 { display, screen })
            } else {
                Self::Unavailable
            }
            #[cfg(not(feature = "x11"))]
            panic!("GDK backend is X11 but winit-gtk was built without the `x11` feature");
        } else {
            Self::Unavailable
        }
    }
}

impl rwh_06::HasDisplayHandle for OwnedDisplayHandle {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        let raw = match *self {
            Self::Wayland { display } => {
                let h = rwh_06::WaylandDisplayHandle::new(display);
                rwh_06::RawDisplayHandle::Wayland(h)
            },
            Self::X11 { display, screen } => {
                let h = rwh_06::XlibDisplayHandle::new(Some(display), screen);
                rwh_06::RawDisplayHandle::Xlib(h)
            },
            Self::Unavailable => return Err(rwh_06::HandleError::Unavailable),
        };

        Ok(unsafe { rwh_06::DisplayHandle::borrow_raw(raw) })
    }
}
