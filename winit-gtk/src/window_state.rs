use std::sync::{Arc, Mutex};

use dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use gtk::prelude::*;

#[derive(Debug)]
struct WindowState {
    scale_factor: f64,
    surface_x: i32,
    surface_y: i32,
    outer_x: i32,
    outer_y: i32,
    surface_width: i32,
    surface_height: i32,
    outer_width: i32,
    outer_height: i32,
}

impl WindowState {
    pub fn new(window: &gtk::ApplicationWindow) -> Self {
        let scale_factor = window.scale_factor() as f64;

        let (inner_x, inner_y) = window.position();
        let (surface_width, surface_height) = window.size();

        let mut surface_x = 0;
        let mut surface_y = 0;
        let mut outer_x = inner_x;
        let mut outer_y = inner_y;
        let mut outer_width = surface_width;
        let mut outer_height = surface_height;

        if let Some(window) = window.window() {
            let frame = window.frame_extents();
            outer_x = frame.x();
            outer_y = frame.y();
            outer_width = frame.width() as _;
            outer_height = frame.height() as _;
            surface_x = inner_x - outer_x;
            surface_y = inner_y - outer_y;
        }

        Self {
            scale_factor,
            surface_x,
            surface_y,
            outer_x,
            outer_y,
            surface_width,
            surface_height,
            outer_width,
            outer_height,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedWindowState(Arc<Mutex<WindowState>>);

impl SharedWindowState {
    pub fn new(window: &gtk::ApplicationWindow) -> Self {
        Self(Arc::new(Mutex::new(WindowState::new(window))))
    }

    pub fn scale_factor(&self) -> f64 {
        self.0.lock().unwrap().scale_factor
    }

    pub fn update_scale_factor(&self, scale_factor: f64) -> bool {
        let mut state = self.0.lock().unwrap();
        maybe_update(&mut state.scale_factor, scale_factor)
    }

    pub fn surface_position(&self) -> PhysicalPosition<i32> {
        let state = self.0.lock().unwrap();
        LogicalPosition::new(state.surface_x, state.surface_y).to_physical(state.scale_factor)
    }

    pub fn outer_position(&self) -> PhysicalPosition<i32> {
        let state = self.0.lock().unwrap();
        LogicalPosition::new(state.outer_x, state.outer_y).to_physical(state.scale_factor)
    }

    pub fn surface_size(&self) -> PhysicalSize<u32> {
        let state = self.0.lock().unwrap();
        LogicalSize::new(state.surface_width, state.surface_height).to_physical(state.scale_factor)
    }

    pub fn outer_size(&self) -> PhysicalSize<u32> {
        let state = self.0.lock().unwrap();
        LogicalSize::new(state.outer_width, state.outer_height).to_physical(state.scale_factor)
    }

    pub fn update_position_and_size(
        &self,
        surface_x: i32,
        surface_y: i32,
        surface_width: i32,
        surface_height: i32,
        outer_x: i32,
        outer_y: i32,
        outer_width: i32,
        outer_height: i32,
    ) -> (bool, bool) {
        let mut state = self.0.lock().unwrap();
        let mut surface_size_changed = false;
        let mut outer_position_changed = false;

        maybe_update(&mut state.surface_x, surface_x);
        maybe_update(&mut state.surface_y, surface_y);
        surface_size_changed |= maybe_update(&mut state.surface_width, surface_width);
        surface_size_changed |= maybe_update(&mut state.surface_height, surface_height);

        outer_position_changed |= maybe_update(&mut state.outer_x, outer_x);
        outer_position_changed |= maybe_update(&mut state.outer_y, outer_y);
        maybe_update(&mut state.outer_width, outer_width);
        maybe_update(&mut state.outer_height, outer_height);

        (surface_size_changed, outer_position_changed)
    }
}

#[inline]
fn maybe_update<T: PartialEq>(current: &mut T, new: T) -> bool {
    if *current != new {
        *current = new;
        true
    } else {
        false
    }
}
