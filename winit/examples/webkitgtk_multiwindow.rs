use std::error::Error;

#[cfg(all(any(x11_platform, wayland_platform), feature = "glib"))]
fn main() -> Result<(), Box<dyn Error>> {
    #[path = "util/fill.rs"]
    mod fill;
    #[path = "util/tracing.rs"]
    mod tracing_init;

    use std::sync::mpsc;

    use webkit6::prelude::*;
    use webkit6::{WebView, glib, gtk};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
    use winit::window::{Window, WindowAttributes, WindowId};

    tracing_init::init();

    #[derive(Debug)]
    enum Msg {
        Ping(&'static str),
        GtkClosed,
    }

    #[derive(Default)]
    struct App {
        winit_window: Option<Box<dyn Window>>,
        winit_closed: bool,
        gtk_window: Option<gtk::Window>,
        gtk_closed: bool,
        proxy: Option<EventLoopProxy>,
        rx: Option<mpsc::Receiver<Msg>>,
        ping_count: u64,
    }

    impl App {
        fn maybe_exit(&mut self, el: &dyn ActiveEventLoop) {
            if self.winit_closed && self.gtk_closed {
                el.exit();
            }
        }
    }

    impl ApplicationHandler for App {
        fn can_create_surfaces(&mut self, el: &dyn ActiveEventLoop) {
            el.set_control_flow(ControlFlow::Wait);

            self.winit_window = match el.create_window(WindowAttributes::default()) {
                Ok(w) => {
                    w.set_title("winit window (idle)");
                    Some(w)
                },
                Err(e) => {
                    eprintln!("failed to create winit window: {e}");
                    el.exit();
                    return;
                },
            };

            let proxy = el.create_proxy();
            let (tx, rx) = mpsc::channel::<Msg>();
            self.proxy = Some(proxy.clone());
            self.rx = Some(rx);

            if !gtk::is_initialized() {
                if let Err(e) = gtk::init() {
                    eprintln!("gtk::init failed: {e}");
                    el.exit();
                    return;
                }
            }

            let gtk_window = gtk::Window::builder()
                .title("GTK/WebKit window (driven by winit/calloop)")
                .default_width(900)
                .default_height(600)
                .build();

            let button = gtk::Button::with_label("Ping winit (updates title)");

            let webview = WebView::new();
            webview.set_hexpand(true);
            webview.set_vexpand(true);
            webview.load_uri("https://crates.io/");

            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
            vbox.set_hexpand(true);
            vbox.set_vexpand(true);
            vbox.append(&button);
            vbox.append(&webview);

            gtk_window.set_child(Some(&vbox));
            gtk_window.present();

            {
                let tx = tx.clone();
                let proxy = proxy.clone();
                button.connect_clicked(move |_| {
                    let _ = tx.send(Msg::Ping("button"));
                    proxy.wake_up();
                });
            }

            {
                let tx = tx.clone();
                let proxy = proxy.clone();
                gtk_window.connect_close_request(move |_| {
                    let _ = tx.send(Msg::GtkClosed);
                    proxy.wake_up();
                    glib::Propagation::Proceed
                });
            }

            self.gtk_window = Some(gtk_window);
        }

        fn proxy_wake_up(&mut self, el: &dyn ActiveEventLoop) {
            let msgs: Vec<_> = match self.rx.as_ref() {
                Some(rx) => rx.try_iter().collect(),
                None => return,
            };

            for msg in msgs {
                match msg {
                    Msg::Ping(src) => {
                        self.ping_count += 1;
                        if let Some(win) = self.winit_window.as_ref() {
                            win.set_title(&format!(
                                "winit window (ping #{}, from {})",
                                self.ping_count, src
                            ));
                            win.request_redraw();
                        }
                    },
                    Msg::GtkClosed => {
                        self.gtk_closed = true;
                        self.maybe_exit(el);
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
                    self.maybe_exit(el);
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
    event_loop.run_app(App::default())?;
    Ok(())
}

#[cfg(any(all(not(x11_platform), not(wayland_platform)), not(feature = "glib")))]
fn main() -> Result<(), Box<dyn Error>> {
    println!("This example is only supported on X11/Wayland platforms with glib feature enabled.");
    Ok(())
}
