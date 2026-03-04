use gtk::gdk;
use gtk::prelude::*;

#[derive(Debug, Clone)]
pub enum Toplevel {
    Gtk(gtk::ApplicationWindow),
    Gdk(gdk::Toplevel, gdk::ToplevelLayout, gdk::Surface),
}

impl Toplevel {
    pub fn destroy(&self) {
        match self {
            Self::Gtk(w) => w.destroy(),
            Self::Gdk(s, ..) => s.destroy(),
        }
    }

    pub fn set_title(&self, title: &str) {
        match self {
            Self::Gtk(w) => w.set_title(Some(title)),
            Self::Gdk(s, ..) => s.set_title(title),
        }
    }

    pub fn set_visible(&self, visible: bool) {
        match self {
            Self::Gtk(w) => {
                w.set_visible(visible);
                if visible {
                    w.present();
                }
            },
            Self::Gdk(..) => todo!(),
        }
    }

    pub fn set_resizable(&self, resizable: bool) {
        match self {
            Self::Gtk(w) => w.set_resizable(resizable),
            Self::Gdk(..) => todo!(),
        }
    }
}
