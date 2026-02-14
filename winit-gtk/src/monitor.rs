use std::borrow::Cow;

use dpi::{LogicalPosition, PhysicalPosition};
use gtk::{gdk, prelude::*};
use winit_core::monitor::{MonitorHandleProvider, VideoMode};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonitorHandle(pub(crate) gdk::Monitor);

unsafe impl Send for MonitorHandle {}
unsafe impl Sync for MonitorHandle {}

impl MonitorHandleProvider for MonitorHandle {
    fn id(&self) -> u128 {
        self.native_id() as _
    }

    fn native_id(&self) -> u64 {
        self.0.as_ptr() as _
    }

    fn name(&self) -> Option<Cow<'_, str>> {
        self.0.model().map(|s| Cow::Owned(s.to_string()))
    }

    fn position(&self) -> Option<PhysicalPosition<i32>> {
        let rect = self.0.geometry();
        let logical = LogicalPosition::new(rect.x(), rect.y());
        Some(logical.to_physical(self.scale_factor()))
    }

    fn scale_factor(&self) -> f64 {
        self.0.scale_factor() as _
    }

    fn current_video_mode(&self) -> Option<VideoMode> {
        None
    }

    fn video_modes(&self) -> Box<dyn Iterator<Item = VideoMode>> {
        Box::new(std::iter::empty())
    }
}
