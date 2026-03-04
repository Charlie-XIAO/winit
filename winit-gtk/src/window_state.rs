use std::sync::{Arc, Mutex};

use dpi::{LogicalSize, PhysicalSize};
use gtk::gdk;
use gtk::prelude::*;

#[derive(Debug)]
struct WindowState {
    scale_factor: f64,
    surface_width: i32,
    surface_height: i32,
    outer_width: i32,
    outer_height: i32,
}

#[derive(Debug, Clone)]
pub struct SharedWindowState(Arc<Mutex<WindowState>>);

impl SharedWindowState {
    pub fn new_gtk(window: &gtk::ApplicationWindow) -> Self {
        Self(Arc::new(Mutex::new(WindowState {
            scale_factor: window.scale_factor() as _,
            surface_width: window.width(),
            surface_height: window.height(),
            outer_width: window.width(),
            outer_height: window.height(),
        })))
    }

    pub fn new_gdk(toplevel: &gdk::Toplevel) -> Self {
        Self(Arc::new(Mutex::new(WindowState {
            scale_factor: toplevel.scale_factor() as _,
            surface_width: toplevel.width(),
            surface_height: toplevel.height(),
            outer_width: toplevel.width(),
            outer_height: toplevel.height(),
        })))
    }

    pub fn scale_factor(&self) -> f64 {
        self.0.lock().unwrap().scale_factor
    }

    pub fn update_scale_factor(&self, scale_factor: f64) {
        let mut state = self.0.lock().unwrap();
        state.scale_factor = scale_factor;
    }

    pub fn surface_size(&self) -> PhysicalSize<u32> {
        let state = self.0.lock().unwrap();
        LogicalSize::new(state.surface_width, state.surface_height).to_physical(state.scale_factor)
    }

    pub fn update_surface_size(&self, width: i32, height: i32) {
        let mut state = self.0.lock().unwrap();
        state.surface_width = width;
        state.surface_height = height;
    }

    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let state = self.0.lock().unwrap();
        LogicalSize::new(state.outer_width, state.outer_height).to_physical(state.scale_factor)
    }

    pub fn update_outer_size(&self, width: i32, height: i32) {
        let mut state = self.0.lock().unwrap();
        state.outer_width = width;
        state.outer_height = height;
    }
}
