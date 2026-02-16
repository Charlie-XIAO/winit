use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use gtk::prelude::*;

#[derive(Debug)]
pub struct WindowState {
    scale_factor: AtomicI32,
    surface_x: AtomicI32,
    surface_y: AtomicI32,
    outer_x: AtomicI32,
    outer_y: AtomicI32,
    surface_width: AtomicU32,
    surface_height: AtomicU32,
    outer_width: AtomicU32,
    outer_height: AtomicU32,
}

impl WindowState {
    pub fn new(window: &gtk::ApplicationWindow) -> Self {
        let scale_factor = window.scale_factor();

        let mut surface_x = 0;
        let mut surface_y = 0;
        let mut outer_x = 0;
        let mut outer_y = 0;

        let (surface_width, surface_height) = window.size();
        let mut outer_width = surface_width;
        let mut outer_height = surface_height;

        if let Some(window) = window.window() {
            let frame = window.frame_extents();
            outer_x = frame.x();
            outer_y = frame.y();
            outer_width = frame.width();
            outer_height = frame.height();

            let (_, sx, sy) = window.origin();
            surface_x = sx - outer_x;
            surface_y = sy - outer_y;
        }

        Self {
            scale_factor: AtomicI32::new(scale_factor),
            surface_x: AtomicI32::new(surface_x),
            surface_y: AtomicI32::new(surface_y),
            outer_x: AtomicI32::new(outer_x),
            outer_y: AtomicI32::new(outer_y),
            surface_width: AtomicU32::new(surface_width as _),
            surface_height: AtomicU32::new(surface_height as _),
            outer_width: AtomicU32::new(outer_width as _),
            outer_height: AtomicU32::new(outer_height as _),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor.load(Ordering::Acquire) as _
    }

    pub fn set_scale_factor(&self, scale_factor: i32) {
        self.scale_factor.store(scale_factor, Ordering::Release);
    }

    pub fn surface_position(&self) -> PhysicalPosition<i32> {
        LogicalPosition::new(
            self.surface_x.load(Ordering::Acquire),
            self.surface_y.load(Ordering::Acquire),
        )
        .to_physical(self.scale_factor())
    }

    pub fn set_surface_position(&self, x: i32, y: i32) {
        self.surface_x.store(x, Ordering::Release);
        self.surface_y.store(y, Ordering::Release);
    }

    pub fn outer_position(&self) -> PhysicalPosition<i32> {
        LogicalPosition::new(
            self.outer_x.load(Ordering::Acquire),
            self.outer_y.load(Ordering::Acquire),
        )
        .to_physical(self.scale_factor())
    }

    pub fn set_outer_position(&self, x: i32, y: i32) {
        self.outer_x.store(x, Ordering::Release);
        self.outer_y.store(y, Ordering::Release);
    }

    pub fn surface_size(&self) -> PhysicalSize<u32> {
        LogicalSize::new(
            self.surface_width.load(Ordering::Acquire),
            self.surface_height.load(Ordering::Acquire),
        )
        .to_physical(self.scale_factor())
    }

    pub fn set_surface_size(&self, width: u32, height: u32) {
        self.surface_width.store(width, Ordering::Release);
        self.surface_height.store(height, Ordering::Release);
    }

    pub fn outer_size(&self) -> PhysicalSize<u32> {
        LogicalSize::new(
            self.outer_width.load(Ordering::Acquire),
            self.outer_height.load(Ordering::Acquire),
        )
        .to_physical(self.scale_factor())
    }

    pub fn set_outer_size(&self, width: u32, height: u32) {
        self.outer_width.store(width, Ordering::Release);
        self.outer_height.store(height, Ordering::Release);
    }
}
