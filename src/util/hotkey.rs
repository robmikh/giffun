use std::sync::atomic::{AtomicI32, Ordering};
use windows::{
    core::Result,
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS},
            WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, WM_HOTKEY},
        },
    },
};

static mut HOT_KEY_ID: AtomicI32 = AtomicI32::new(0);

pub struct HotKey {
    id: i32,
}

impl HotKey {
    pub fn new(modifiers: HOT_KEY_MODIFIERS, key: u32) -> Result<Self> {
        let id = unsafe { HOT_KEY_ID.fetch_add(1, Ordering::SeqCst) + 1 };
        unsafe {
            RegisterHotKey(HWND(0), id, modifiers, key).ok()?;
        }
        Ok(Self { id })
    }
}

impl Drop for HotKey {
    fn drop(&mut self) {
        unsafe { UnregisterHotKey(HWND(0), self.id).ok().unwrap() }
    }
}

pub fn pump_messages<F: FnMut() -> Result<bool>>(
    modifiers: HOT_KEY_MODIFIERS,
    key: u32,
    mut hot_key_callback: F,
) -> Result<()> {
    let _hot_key = HotKey::new(modifiers, key)?;
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND(0), 0, 0).into() {
            if message.message == WM_HOTKEY {
                if hot_key_callback()? {
                    break;
                }
            }
            DispatchMessageW(&mut message);
        }
    }
    Ok(())
}
