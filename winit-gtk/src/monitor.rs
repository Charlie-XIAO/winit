use std::{borrow::Cow, ops::Deref};

use dpi::PhysicalPosition;
use gtk::gdk;
use winit_core::monitor::{MonitorHandleProvider, VideoMode};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonitorHandle(gdk::Monitor);

// TODO: MonitorHandleProvider requires Send + Sync, but are we safe?
unsafe impl Send for MonitorHandle {}
unsafe impl Sync for MonitorHandle {}

impl Deref for MonitorHandle {
    type Target = gdk::Monitor;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl MonitorHandleProvider for MonitorHandle {
    fn id(&self) -> u128 {
        todo!()
    }

    fn native_id(&self) -> u64 {
        todo!()
    }

    fn name(&self) -> Option<Cow<'_, str>> {
        todo!()
    }

    fn position(&self) -> Option<PhysicalPosition<i32>> {
        todo!()
    }

    fn scale_factor(&self) -> f64 {
        todo!()
    }

    fn current_video_mode(&self) -> Option<VideoMode> {
        todo!()
    }

    fn video_modes(&self) -> Box<dyn Iterator<Item = VideoMode>> {
        todo!()
    }
}
