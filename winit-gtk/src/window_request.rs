use std::sync::Arc;

use dpi::{LogicalPosition, LogicalSize};
use gtk::{cairo, gdk, glib, prelude::*};
use winit_core::{event::WindowEvent, window::WindowId};

use crate::{
    event_loop::{EventLoopWindow, EventLoopWindows, QueuedEvent},
    window_state::WindowState,
};

#[non_exhaustive]
pub enum WindowRequest {
    Title(String),
    Visible(bool),
    Resizable(bool),
    Destroy,
    WithGtkWindow(Box<dyn FnOnce(&gtk::ApplicationWindow) + Send + 'static>),
    WithDefaultVbox(Box<dyn FnOnce(Option<&gtk::Box>) + Send + 'static>),
    WireUpEvents { transparent_draw: bool, pointer_moved: bool, fullscreen: bool },
}

pub async fn handle_window_requests(
    windows: EventLoopWindows,
    window_requests_rx: async_channel::Receiver<(WindowId, WindowRequest)>,
    events_tx: crossbeam_channel::Sender<QueuedEvent>,
    redraw_tx: crossbeam_channel::Sender<WindowId>,
) {
    while let Ok((id, request)) = window_requests_rx.recv().await {
        if matches!(request, WindowRequest::Destroy) {
            if let Some(window) = windows.borrow_mut().remove(&id) {
                unsafe {
                    window.window.destroy();
                }
            }
            continue;
        }

        if let Some(window) = windows.borrow().get(&id).cloned() {
            let EventLoopWindow { window, default_vbox, state } = window;

            match request {
                WindowRequest::Title(title) => {
                    window.set_title(&title);
                },
                WindowRequest::Visible(visible) => {
                    if visible {
                        window.show_all();
                    } else {
                        window.hide();
                    }
                },
                WindowRequest::Resizable(resizable) => {
                    window.set_resizable(resizable);
                },
                WindowRequest::WithGtkWindow(f) => {
                    f(&window);
                },
                WindowRequest::WithDefaultVbox(f) => {
                    f(default_vbox.as_ref());
                },
                WindowRequest::WireUpEvents { transparent_draw, pointer_moved, fullscreen } => {
                    handle_wire_up_events(
                        id,
                        &window,
                        state,
                        events_tx.clone(),
                        redraw_tx.clone(),
                        transparent_draw,
                        pointer_moved,
                        fullscreen,
                    );
                },
                _ => unreachable!(),
            }
        }
    }
}

fn handle_wire_up_events(
    id: WindowId,
    window: &gtk::ApplicationWindow,
    state: Arc<WindowState>,
    events_tx: crossbeam_channel::Sender<QueuedEvent>,
    redraw_tx: crossbeam_channel::Sender<WindowId>,
    transparent_draw: bool,
    pointer_moved: bool,
    fullscreen: bool,
) {
    let _ = pointer_moved;
    let _ = fullscreen;

    window.add_events(
        gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::BUTTON1_MOTION_MASK
            | gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::TOUCH_MASK
            | gdk::EventMask::STRUCTURE_MASK
            | gdk::EventMask::FOCUS_CHANGE_MASK
            | gdk::EventMask::SCROLL_MASK,
    );

    // Handle when the scale factor of the window changes
    {
        let state = state.clone();
        window.connect_scale_factor_notify(move |w| {
            state.set_scale_factor(w.scale_factor());
        });
    }

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L472-L483

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L485-L511

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L512-L546

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L547-L586

    // Handle when user requests to close the window
    {
        let tx = events_tx.clone();
        window.connect_delete_event(move |_, _| {
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::CloseRequested })
            {
                tracing::warn!("Failed to send WindowEvent::CloseRequested: {e}");
            }
            glib::Propagation::Stop
        });
    }

    // Handle when the size or position of the window changes
    {
        let tx = events_tx.clone();
        let state = state.clone();
        window.connect_configure_event(move |window, event| {
            let mut surface_x = 0;
            let mut surface_y = 0;
            let mut outer_x = 0;
            let mut outer_y = 0;

            let (surface_width, surface_height) = event.size();
            let mut outer_width = surface_width;
            let mut outer_height = surface_height;

            if let Some(window) = window.window() {
                let frame = window.frame_extents();
                outer_x = frame.x();
                outer_y = frame.y();
                outer_width = frame.width() as _;
                outer_height = frame.height() as _;

                let (_, sx, sy) = window.origin();
                surface_x = sx - outer_x;
                surface_y = sy - outer_y;
            }

            state.set_surface_position(surface_x, surface_y);
            state.set_surface_size(surface_width, surface_height);
            state.set_outer_position(outer_x, outer_y);
            state.set_outer_size(outer_width, outer_height);

            let scale_factor = window.scale_factor() as f64;

            let pos = LogicalPosition::new(outer_x, outer_y).to_physical(scale_factor);
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::Moved(pos) }) {
                tracing::warn!("Failed to send WindowEvent::Moved: {e}");
            }

            let size = LogicalSize::new(surface_width, surface_height).to_physical(scale_factor);
            if let Err(e) =
                tx.send(QueuedEvent::Window { id, event: WindowEvent::SurfaceResized(size) })
            {
                tracing::warn!("Failed to send WindowEvent::SurfaceResized: {e}");
            }

            false // Propagate the event further
        });
    }

    // Handle when the keyboard focus enters or leaves the window
    {
        let tx = events_tx.clone();
        window.connect_focus_in_event(move |_, _| {
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::Focused(true) }) {
                tracing::warn!("Failed to send WindowEvent::Focused: {e}");
            }
            glib::Propagation::Proceed
        });

        let tx = events_tx.clone();
        window.connect_focus_out_event(move |_, _| {
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::Focused(false) })
            {
                tracing::warn!("Failed to send WindowEvent::Focused: {e}");
            }
            glib::Propagation::Proceed
        });
    }

    // Handle when the window is destroyed
    {
        let tx = events_tx.clone();
        window.connect_destroy(move |_| {
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::Destroyed }) {
                tracing::warn!("Failed to send WindowEvent::Destroyed: {e}");
            }
        });
    }

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L672-L686

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L688-L708

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L710-L721

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L724-L747

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L749-L773

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L775-L793

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L795-L836

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L838-L853

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L855-L861

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L863-L867

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L869-L899

    // Handle when the window needs to be redrawn
    {
        let tx = redraw_tx.clone();
        window.connect_draw(move |_, cr| {
            if let Err(e) = tx.send(id) {
                tracing::warn!("Failed to send draw event: {e}");
            }

            // TODO: TAO src/platform_impl/linux/event_loop.rs:L902-L937
            // Implement when background_color attribute is added, see also
            // https://github.com/tauri-apps/tao/pull/995
            if transparent_draw {
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
                cr.set_operator(cairo::Operator::Source);
                let _ = cr.paint();
                cr.set_operator(cairo::Operator::Over);
            }

            glib::Propagation::Proceed
        });
    }
}
