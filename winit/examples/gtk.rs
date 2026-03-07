use std::error::Error;

#[cfg(all(any(x11_platform, wayland_platform), feature = "glib"))]
fn main() -> Result<(), Box<dyn Error>> {
    #[path = "util/fill.rs"]
    mod fill;
    #[path = "util/tracing.rs"]
    mod tracing_init;

    use std::sync::mpsc;

    use gtk::glib;
    use gtk::prelude::*;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
    use winit::window::{Window, WindowAttributes, WindowId};

    tracing_init::init();

    #[derive(Debug)]
    enum UserEvent {
        Ping,
        GtkClosed,
    }

    #[derive(Default)]
    struct Application {
        winit_window: Option<Box<dyn Window>>,
        winit_closed: bool,
        gtk_window: Option<gtk::Window>,
        gtk_closed: bool,
        proxy: Option<EventLoopProxy>,
        rx: Option<mpsc::Receiver<UserEvent>>,
        ping_count: u64,
    }

    impl ApplicationHandler for Application {
        fn can_create_surfaces(&mut self, el: &dyn ActiveEventLoop) {
            el.set_control_flow(ControlFlow::Wait);

            let attributes = WindowAttributes::default().with_title("winit (idle)");
            self.winit_window = match el.create_window(attributes) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("Failed to create winit window: {e}");
                    el.exit();
                    return;
                },
            };

            let proxy = el.create_proxy();
            let (tx, rx) = mpsc::channel();
            self.proxy = Some(proxy.clone());
            self.rx = Some(rx);

            if !gtk::is_initialized() {
                if let Err(e) = gtk::init() {
                    eprintln!("Failed to initialize GTK: {e}");
                    el.exit();
                    return;
                }
            }

            let gtk_window =
                gtk::Window::builder().title("GTK").default_width(300).default_height(200).build();

            let button = gtk::Button::with_label("Ping winit (updates title)");
            gtk_window.set_child(Some(&button));
            gtk_window.present();

            {
                let tx = tx.clone();
                let proxy = proxy.clone();
                button.connect_clicked(move |_| {
                    let _ = tx.send(UserEvent::Ping);
                    proxy.wake_up();
                });
            }

            {
                let tx = tx.clone();
                let proxy = proxy.clone();
                gtk_window.connect_close_request(move |_| {
                    let _ = tx.send(UserEvent::GtkClosed);
                    proxy.wake_up();
                    glib::Propagation::Proceed
                });
            }

            self.gtk_window = Some(gtk_window);
        }

        fn proxy_wake_up(&mut self, el: &dyn ActiveEventLoop) {
            let events: Vec<_> = match self.rx.as_ref() {
                Some(rx) => rx.try_iter().collect(),
                None => return,
            };

            for event in events {
                match event {
                    UserEvent::Ping => {
                        self.ping_count += 1;
                        if let Some(win) = self.winit_window.as_ref() {
                            win.set_title(&format!("winit (ping #{})", self.ping_count));
                            win.request_redraw();
                        }
                    },
                    UserEvent::GtkClosed => {
                        self.gtk_closed = true;
                        if self.winit_closed {
                            el.exit();
                        }
                    },
                }
            }
        }

        fn window_event(&mut self, el: &dyn ActiveEventLoop, _id: WindowId, event: WindowEvent) {
            match event {
                WindowEvent::CloseRequested => {
                    self.winit_closed = true;
                    if let Some(w) = self.gtk_window.take() {
                        w.close();
                    } else {
                        self.gtk_closed = true;
                    }
                    if self.gtk_closed {
                        el.exit();
                    }
                },
                WindowEvent::SurfaceResized(_) => {
                    if let Some(win) = self.winit_window.as_ref() {
                        win.request_redraw();
                    }
                },
                WindowEvent::RedrawRequested => {
                    let win = self.winit_window.as_ref().unwrap();
                    win.pre_present_notify();
                    fill::fill_window(win.as_ref());
                },
                _ => {},
            }
        }
    }

    let event_loop = EventLoop::new()?;
    let app = Application::default();
    event_loop.run_app(app)?;
    Ok(())
}

#[cfg(any(all(not(x11_platform), not(wayland_platform)), not(feature = "glib")))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("This example is only supported on X11/Wayland platforms with glib feature enabled.");
    Ok(())
}
