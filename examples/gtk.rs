use std::error::Error;

#[cfg(all(any(x11_platform, wayland_platform), feature = "glib"))]
fn main() -> Result<(), Box<dyn Error>> {
    #[path = "util/fill.rs"]
    mod fill;
    #[path = "util/tracing.rs"]
    mod tracing_init;

    use gtk::glib;
    use gtk::prelude::*;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
    use winit::window::{Window, WindowId};

    tracing_init::init();

    #[derive(Debug)]
    enum UserEvent {
        Ping,
        GtkClosed,
    }

    struct Application {
        winit_window: Option<Window>,
        winit_closed: bool,
        gtk_window: Option<gtk::Window>,
        gtk_closed: bool,
        proxy: EventLoopProxy<UserEvent>,
        ping_count: u64,
    }

    impl Application {
        fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
            Self {
                winit_window: None,
                winit_closed: false,
                gtk_window: None,
                gtk_closed: false,
                proxy,
                ping_count: 0,
            }
        }
    }

    impl ApplicationHandler<UserEvent> for Application {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            el.set_control_flow(ControlFlow::Wait);

            let attributes = Window::default_attributes().with_title("winit (idle)");
            self.winit_window = match el.create_window(attributes) {
                Ok(w) => Some(w),
                Err(e) => {
                    eprintln!("Failed to create winit window: {e}");
                    el.exit();
                    return;
                },
            };

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
                let proxy = self.proxy.clone();
                button.connect_clicked(move |_| {
                    let _ = proxy.send_event(UserEvent::Ping);
                });
            }

            {
                let proxy = self.proxy.clone();
                gtk_window.connect_close_request(move |_| {
                    let _ = proxy.send_event(UserEvent::GtkClosed);
                    glib::Propagation::Proceed
                });
            }

            self.gtk_window = Some(gtk_window);
        }

        fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
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

        fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                WindowEvent::Resized(_) => {
                    if let Some(win) = self.winit_window.as_ref() {
                        win.request_redraw();
                    }
                },
                WindowEvent::RedrawRequested => {
                    let win = self.winit_window.as_ref().unwrap();
                    win.pre_present_notify();
                    fill::fill_window(win);
                },
                _ => {},
            }
        }
    }

    let event_loop = EventLoop::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut app = Application::new(proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(any(all(not(x11_platform), not(wayland_platform)), not(feature = "glib")))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("This example is only supported on X11/Wayland platforms with glib feature enabled.");
    Ok(())
}
