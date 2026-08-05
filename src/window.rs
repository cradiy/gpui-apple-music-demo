use gpui::Window;

#[cfg(target_os = "windows")]
fn native_handle(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let window_handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(window_handle) = window_handle.as_raw() else {
        return None;
    };

    Some(window_handle.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

#[cfg(target_os = "windows")]
pub fn remove_frame(window: &Window) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };

    let Some(hwnd) = native_handle(window) else {
        return;
    };

    unsafe {
        // Keep GPUI's native resizable/maximizable styles so Windows Snap remains available,
        // and suppress only the visible one-pixel DWM border.
        let border_color = DWMWA_COLOR_NONE;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            (&border_color as *const _) as *const core::ffi::c_void,
            core::mem::size_of_val(&border_color) as u32,
        );

        // DWM owns the outer Windows corner. The GPUI root skips its own corner on Windows,
        // avoiding two different radii clipping the same window edge.
        let corner_preference = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corner_preference as *const _) as *const core::ffi::c_void,
            core::mem::size_of_val(&corner_preference) as u32,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn remove_frame(_: &Window) {}
