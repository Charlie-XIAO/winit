//! The Wayland window.

use dpi::{PhysicalInsets, PhysicalPosition, PhysicalSize, Position, Size};
use winit_core::cursor::Cursor;
use winit_core::error::RequestError;
use winit_core::icon::Icon;
use winit_core::monitor::{Fullscreen, MonitorHandle as CoreMonitorHandle};
use winit_core::window::{
    CursorGrabMode, ImeCapabilities, ImeRequest, ImeRequestError, ResizeDirection, Theme,
    UserAttentionType, Window as CoreWindow, WindowAttributes, WindowButtons, WindowId,
    WindowLevel,
};

use crate::event_loop::ActiveEventLoop;

/// The GTK window.
#[derive(Debug)]
pub struct Window {}

impl Window {
    pub(crate) fn new(
        event_loop_window_target: &ActiveEventLoop,
        mut attributes: WindowAttributes,
    ) -> Result<Self, RequestError> {
        todo!()
    }

    pub(crate) fn default_vbox(&self) -> Option<&gtk::Box> {
        todo!()
    }

    pub(crate) fn set_skip_taskbar(&self, skip_taskbar: bool) {
        todo!()
    }

    pub(crate) fn set_badge_count(&self, count: Option<i64>, desktop_filename: Option<String>) {
        todo!()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        todo!()
    }
}

impl rwh_06::HasWindowHandle for Window {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        todo!()
    }
}

impl rwh_06::HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<rwh_06::DisplayHandle<'_>, rwh_06::HandleError> {
        todo!()
    }
}

impl CoreWindow for Window {
    fn id(&self) -> WindowId {
        todo!()
    }

    fn scale_factor(&self) -> f64 {
        todo!()
    }

    fn request_redraw(&self) {
        todo!()
    }

    fn pre_present_notify(&self) {
        todo!()
    }

    fn reset_dead_keys(&self) {
        todo!()
    }

    fn surface_position(&self) -> PhysicalPosition<i32> {
        todo!()
    }

    fn outer_position(&self) -> Result<PhysicalPosition<i32>, RequestError> {
        todo!()
    }

    fn set_outer_position(&self, position: Position) {
        todo!()
    }

    fn surface_size(&self) -> PhysicalSize<u32> {
        todo!()
    }

    fn request_surface_size(&self, size: Size) -> Option<PhysicalSize<u32>> {
        todo!()
    }

    fn outer_size(&self) -> PhysicalSize<u32> {
        todo!()
    }

    fn safe_area(&self) -> PhysicalInsets<u32> {
        todo!()
    }

    fn set_min_surface_size(&self, min_size: Option<Size>) {
        todo!()
    }

    fn set_max_surface_size(&self, max_size: Option<Size>) {
        todo!()
    }

    fn surface_resize_increments(&self) -> Option<PhysicalSize<u32>> {
        todo!()
    }

    fn set_surface_resize_increments(&self, increments: Option<Size>) {
        todo!()
    }

    fn set_title(&self, title: &str) {
        todo!()
    }

    fn set_transparent(&self, transparent: bool) {
        todo!()
    }

    fn set_blur(&self, blur: bool) {
        todo!()
    }

    fn set_visible(&self, visible: bool) {
        todo!()
    }

    fn is_visible(&self) -> Option<bool> {
        todo!()
    }

    fn set_resizable(&self, resizable: bool) {
        todo!()
    }

    fn is_resizable(&self) -> bool {
        todo!()
    }

    fn set_enabled_buttons(&self, buttons: WindowButtons) {
        todo!()
    }

    fn enabled_buttons(&self) -> WindowButtons {
        todo!()
    }

    fn set_minimized(&self, minimized: bool) {
        todo!()
    }

    fn is_minimized(&self) -> Option<bool> {
        todo!()
    }

    fn set_maximized(&self, maximized: bool) {
        todo!()
    }

    fn is_maximized(&self) -> bool {
        todo!()
    }

    fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        todo!()
    }

    fn fullscreen(&self) -> Option<Fullscreen> {
        todo!()
    }

    fn set_decorations(&self, decorations: bool) {
        todo!()
    }

    fn is_decorated(&self) -> bool {
        todo!()
    }

    fn set_window_level(&self, level: WindowLevel) {
        todo!()
    }

    fn set_window_icon(&self, window_icon: Option<Icon>) {
        todo!()
    }

    fn request_ime_update(&self, request: ImeRequest) -> Result<(), ImeRequestError> {
        todo!()
    }

    fn ime_capabilities(&self) -> Option<ImeCapabilities> {
        todo!()
    }

    fn focus_window(&self) {
        todo!()
    }

    fn has_focus(&self) -> bool {
        todo!()
    }

    fn request_user_attention(&self, request_type: Option<UserAttentionType>) {
        todo!()
    }

    fn set_theme(&self, theme: Option<Theme>) {
        todo!()
    }

    fn theme(&self) -> Option<Theme> {
        todo!()
    }

    fn set_content_protected(&self, protected: bool) {
        todo!()
    }

    fn title(&self) -> String {
        todo!()
    }

    fn set_cursor(&self, cursor: Cursor) {
        todo!()
    }

    fn set_cursor_position(&self, position: Position) -> Result<(), RequestError> {
        todo!()
    }

    fn set_cursor_grab(&self, mode: CursorGrabMode) -> Result<(), RequestError> {
        todo!()
    }

    fn set_cursor_visible(&self, visible: bool) {
        todo!()
    }

    fn drag_window(&self) -> Result<(), RequestError> {
        todo!()
    }

    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), RequestError> {
        todo!()
    }

    fn show_window_menu(&self, position: Position) {
        todo!()
    }

    fn set_cursor_hittest(&self, hittest: bool) -> Result<(), RequestError> {
        todo!()
    }

    fn current_monitor(&self) -> Option<CoreMonitorHandle> {
        todo!()
    }

    fn available_monitors(&self) -> Box<dyn Iterator<Item = CoreMonitorHandle>> {
        todo!()
    }

    fn primary_monitor(&self) -> Option<CoreMonitorHandle> {
        todo!()
    }

    fn rwh_06_display_handle(&self) -> &dyn rwh_06::HasDisplayHandle {
        self
    }

    fn rwh_06_window_handle(&self) -> &dyn rwh_06::HasWindowHandle {
        self
    }
}

/// The request from the window to the event loop.
#[non_exhaustive]
#[derive(Debug)]
pub enum WindowRequest {}
