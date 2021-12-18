mod capture;
mod d3d;
mod diff;
mod encoder;
mod handle;
mod hotkey;
mod lut;
mod palette;
mod quantizer;

use robmikh_common::{
    desktop::{
        capture::create_capture_item_for_monitor,
        dispatcher_queue::DispatcherQueueControllerExtensions,
        displays::get_display_handle_from_index,
    },
    universal::d3d::create_d3d_device,
};
use windows::{
    core::Result,
    Graphics::SizeInt32,
    System::{DispatcherQueueController, VirtualKey},
    Win32::{
        Foundation::HWND,
        System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
        UI::{
            Input::KeyboardAndMouse::{MOD_CONTROL, MOD_SHIFT},
            WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, WM_HOTKEY},
        },
    },
};

use crate::{encoder::GifEncoder, hotkey::HotKey, palette::DEFAULT_PALETTE};

fn main() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _controller =
        DispatcherQueueController::create_dispatcher_queue_controller_for_current_thread()?;

    // Get the primary monitor
    let display_handle = get_display_handle_from_index(0).expect("No monitors detected!");
    let capture_item = create_capture_item_for_monitor(display_handle)?;

    // Check to see if we're using the debug layer
    if cfg!(feature = "debug") {
        println!("Using the D3D11 debug layer...");
    }

    // Init d3d11
    let d3d_device = create_d3d_device()?;

    // We're only going to capture part of the screen
    let capture_size = SizeInt32 {
        Width: 1000,
        Height: 1000,
    };

    // Create our palette
    let palette = &DEFAULT_PALETTE;

    // Create our encoder
    let mut encoder = GifEncoder::new(
        &d3d_device,
        palette,
        capture_item,
        capture_size,
        "recording.gif",
    )?;

    // Record
    let mut is_recording = false;
    pump_messages(|| -> Result<bool> {
        Ok(if !is_recording {
            is_recording = true;
            println!("Starting recording...");
            encoder.start()?;
            false
        } else {
            true
        })
    })?;
    println!("Stopping recording...");
    encoder.stop()?;

    Ok(())
}

fn pump_messages<F: FnMut() -> Result<bool>>(mut hot_key_callback: F) -> Result<()> {
    let _hot_key = HotKey::new(MOD_SHIFT | MOD_CONTROL, VirtualKey::R.0 as u32)?;
    println!("Press SHIFT+CTRL+R to start/stop the recording...");
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
