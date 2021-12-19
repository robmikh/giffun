use windows::{
    core::Result,
    Graphics::RectInt32,
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
    },
};

pub fn get_window_rect(window: HWND) -> Result<RectInt32> {
    let mut rect = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )?;
    }
    Ok(RectInt32 {
        X: rect.left,
        Y: rect.top,
        Width: rect.right - rect.left,
        Height: rect.bottom - rect.top,
    })
}
