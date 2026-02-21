use dpi::LogicalSize;
use gtk::glib;
use gtk::prelude::*;
use winit_core::event::WindowEvent;
use winit_core::window::WindowId;

use crate::event_loop::{EventLoopWindow, EventLoopWindows, QueuedEvent};
use crate::window_state::SharedWindowState;

#[non_exhaustive]
pub enum WindowRequest {
    Title(String),
    Visible(bool),
    Resizable(bool),
    Destroy,
    WithGtkWindow(Box<dyn FnOnce(&gtk::ApplicationWindow) + Send + 'static>),
    WithGtkDrawingArea(Box<dyn FnOnce(&gtk::DrawingArea) + Send + 'static>),
    WireUpEvents { fullscreen: bool },
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
                window.window.destroy();
            }
            continue;
        }

        if let Some(window) = windows.borrow().get(&id).cloned() {
            let EventLoopWindow { window, drawing_area, state } = window;

            match request {
                WindowRequest::Title(title) => {
                    window.set_title(Some(&title));
                },
                WindowRequest::Visible(visible) => {
                    window.set_visible(visible);
                    if visible {
                        window.present();
                    }
                },
                WindowRequest::Resizable(resizable) => {
                    window.set_resizable(resizable);
                },
                WindowRequest::WithGtkWindow(f) => {
                    f(&window);
                },
                WindowRequest::WithGtkDrawingArea(f) => {
                    f(&drawing_area);
                },
                WindowRequest::WireUpEvents { fullscreen } => {
                    handle_wire_up_events(
                        id,
                        &window,
                        &drawing_area,
                        state,
                        events_tx.clone(),
                        redraw_tx.clone(),
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
    drawing_area: &gtk::DrawingArea,
    state: SharedWindowState,
    events_tx: crossbeam_channel::Sender<QueuedEvent>,
    redraw_tx: crossbeam_channel::Sender<WindowId>,
    fullscreen: bool,
) {
    let _ = fullscreen;

    // Handle when the scale factor of the window changes
    {
        let state = state.clone();
        window.connect_scale_factor_notify(move |w| {
            state.update_scale_factor(w.scale_factor() as _);
        });
    }

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L472-L483

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L485-L511

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L512-L546

    // TODO: TAO src/platform_impl/linux/event_loop.rs:L547-L586

    // Handle when user requests to close the window
    {
        let tx = events_tx.clone();
        window.connect_close_request(move |_| {
            if let Err(e) = tx.send(QueuedEvent::Window { id, event: WindowEvent::CloseRequested })
            {
                tracing::warn!("Failed to send WindowEvent::CloseRequested: {e}");
            }
            glib::Propagation::Stop
        });
    }

    // Handle when the window is resized
    {
        let state = state.clone();
        if let Some(surface) = window.surface() {
            surface.connect_layout(move |_, width, height| {
                state.update_outer_size(width, height);
            });
        }
    }

    // Handle when the drawing area is resized
    {
        let tx = events_tx.clone();
        let state = state.clone();
        drawing_area.connect_resize(move |drawing_area, width, height| {
            state.update_surface_size(width, height);

            let scale_factor = drawing_area.scale_factor() as f64;
            let size = LogicalSize::new(width, height).to_physical(scale_factor);
            if let Err(e) =
                tx.send(QueuedEvent::Window { id, event: WindowEvent::SurfaceResized(size) })
            {
                tracing::warn!("Failed to send WindowEvent::SurfaceResized: {e}");
            }
        });
    }

    // Handle when the keyboard focus enters or leaves the window
    {
        // TODO
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
        // TODO
    }
}
