mod capture;
mod cli;
mod encoder;
mod util;

use std::path::Path;

use cli::{parse_cli, CaptureType};
use robmikh_common::{
    desktop::{
        capture::{create_capture_item_for_monitor, create_capture_item_for_window},
        dispatcher_queue::DispatcherQueueControllerExtensions,
    },
    universal::d3d::create_d3d_device,
};
use windows::{
    core::Result,
    System::{DispatcherQueueController, VirtualKey},
    Win32::{
        System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
        UI::Input::KeyboardAndMouse::{MOD_CONTROL, MOD_SHIFT},
    },
};

use crate::{
    encoder::{capture_gif_encoder::CaptureGifEncoder, palette::DEFAULT_PALETTE},
    util::hotkey::pump_messages,
};

fn run<P: AsRef<Path>>(capture_type: CaptureType, output_file_path: P) -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _controller =
        DispatcherQueueController::create_dispatcher_queue_controller_for_current_thread()?;

    // Get the capture item
    let capture_item = match capture_type {
        CaptureType::Window(window) => create_capture_item_for_window(window)?,
        CaptureType::Monitor(monitor) => create_capture_item_for_monitor(monitor)?,
    };

    // Check to see if we're using the debug layer
    if cfg!(feature = "debug") {
        println!("Using the D3D11 debug layer...");
    }

    // Init d3d11
    let d3d_device = create_d3d_device()?;

    // Match the size of the capture item
    let capture_size = capture_item.Size()?;

    // Create our palette
    let palette = &DEFAULT_PALETTE;

    // Create our encoder
    let mut encoder = CaptureGifEncoder::new(
        &d3d_device,
        palette,
        capture_item,
        capture_size,
        output_file_path,
    )?;

    // Record
    let mut is_recording = false;
    println!("Press SHIFT+CTRL+R to start/stop the recording...");
    pump_messages(
        MOD_SHIFT | MOD_CONTROL,
        VirtualKey::R.0 as u32,
        || -> Result<bool> {
            Ok(if !is_recording {
                is_recording = true;
                println!("Starting recording...");
                encoder.start()?;
                false
            } else {
                true
            })
        },
    )?;
    println!("Stopping recording...");
    encoder.stop()?;

    Ok(())
}

fn main() -> Result<()> {
    let cli_options = parse_cli()?;
    run(cli_options.capture_type, &cli_options.output_file)?;
    Ok(())
}
