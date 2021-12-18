mod capture;
mod d3d;
mod handle;
mod hotkey;
mod lut;
mod palette;
mod quantizer;

use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use gif::Repeat;
use robmikh_common::{
    desktop::{
        capture::create_capture_item_for_monitor,
        dispatcher_queue::DispatcherQueueControllerExtensions,
        displays::get_display_handle_from_index,
    },
    universal::d3d::{create_d3d_device, create_direct3d_device, get_d3d_interface_from_object},
};
use windows::{
    core::{Handle, Result},
    Graphics::{Capture::Direct3D11CaptureFrame, SizeInt32},
    System::{DispatcherQueueController, VirtualKey},
    Win32::{
        Foundation::{HWND, PWSTR},
        Graphics::{
            Direct3D11::{
                ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA,
                D3D11_TEXTURE1D_DESC, D3D11_USAGE_DEFAULT,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UINT,
        },
        System::{
            Threading::{CreateEventW, SetEvent, WaitForSingleObject, WAIT_OBJECT_0},
            WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
        },
        UI::{
            Input::KeyboardAndMouse::{MOD_CONTROL, MOD_SHIFT},
            WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, WM_HOTKEY},
        },
    },
};

use crate::{
    capture::CaptureFrameGenerator, handle::AutoCloseHandle, hotkey::HotKey, lut::PaletteIndexLUT,
    palette::DEFAULT_PALETTE, quantizer::ColorQuantizer,
};

const INFINITE: u32 = 0xFFFFFFFF;

fn main() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let _controller =
        DispatcherQueueController::create_dispatcher_queue_controller_for_current_thread()?;

    // Get the primary monitor
    let display_handle = get_display_handle_from_index(0).expect("No monitors detected!");
    let capture_item = create_capture_item_for_monitor(display_handle)?;

    // Init d3d11
    let d3d_device = create_d3d_device()?;
    let d3d_context = unsafe {
        let mut d3d_context = None;
        d3d_device.GetImmediateContext(&mut d3d_context);
        d3d_context.unwrap()
    };
    let device = create_direct3d_device(&d3d_device)?;

    // We're only going to capture part of the screen
    let capture_size = SizeInt32 {
        Width: 1000,
        Height: 1000,
    };

    // Create our palette
    let palette = &DEFAULT_PALETTE;
    let mut palette_with_alpha = {
        let mut palette_with_alpha: Vec<u8> = Vec::with_capacity(256 * 4);
        for chunk in palette.chunks(3) {
            palette_with_alpha.push(chunk[0]);
            palette_with_alpha.push(chunk[1]);
            palette_with_alpha.push(chunk[2]);
            palette_with_alpha.push(255);
        }
        palette_with_alpha
    };

    // Create the palette buffer
    let palette_texture = {
        let desc = D3D11_TEXTURE1D_DESC {
            Width: 256,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UINT,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0,
            ..Default::default()
        };
        // TODO: pSysMem shouldn't be *mut _
        let subresource_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: palette_with_alpha.as_mut_ptr() as *mut _,
            ..Default::default()
        };
        unsafe { d3d_device.CreateTexture1D(&desc, &subresource_data)? }
    };

    // Create a 3d texture for our LUT
    let lut = PaletteIndexLUT::new(&d3d_device, &d3d_context, &palette_texture)?;

    // Create our color quantizer
    let quantizer = ColorQuantizer::new(&d3d_device, &d3d_context, lut, capture_size)?;

    // Setup capture
    let mut frame_generator = CaptureFrameGenerator::new(device, capture_item, capture_size, 2)?;
    let capture_session = frame_generator.session();

    // Setup encoder thread
    let start_event = unsafe {
        let start_event =
            CreateEventW(std::ptr::null(), true, false, PWSTR(std::ptr::null_mut())).ok()?;
        AutoCloseHandle(start_event)
    };
    let should_exit = Arc::new(AtomicBool::new(false));
    let encoder_thread = std::thread::spawn({
        let should_exit = should_exit.clone();
        let start_event = start_event.0;
        move || -> Result<()> {
            assert!(unsafe { WaitForSingleObject(start_event, INFINITE) } == WAIT_OBJECT_0);

            // Setup the gif encoder
            let mut image = File::create("recording.gif").unwrap();
            let mut encoder = gif::Encoder::new(
                &mut image,
                capture_size.Width as u16,
                capture_size.Height as u16,
                palette,
            )
            .unwrap();
            encoder.set_repeat(Repeat::Infinite).unwrap();

            let mut last_timestamp = None;
            let mut process_frame = |frame: Direct3D11CaptureFrame| -> Result<()> {
                let bytes = {
                    let frame_texture: ID3D11Texture2D =
                        get_d3d_interface_from_object(&frame.Surface()?)?;
                    quantizer.quantize(&frame_texture)?
                };

                // Build our gif frame
                let mut gif_frame = gif::Frame::from_indexed_pixels(
                    capture_size.Width as u16,
                    capture_size.Height as u16,
                    &bytes,
                    None,
                );
                let timestamp: Duration = if last_timestamp.is_none() {
                    let timestamp = frame.SystemRelativeTime()?;
                    timestamp
                } else {
                    last_timestamp.unwrap()
                }
                .into();
                let current_timestamp: Duration = {
                    let current_timestamp = frame.SystemRelativeTime()?;
                    last_timestamp = Some(current_timestamp);
                    current_timestamp
                }
                .into();
                let frame_delay = current_timestamp - timestamp;
                //println!("delay: {}", frame_delay.as_millis());
                gif_frame.delay = (frame_delay.as_millis() / 10) as u16;

                // Write our frame to disk
                encoder.write_frame(&gif_frame).unwrap();

                Ok(())
            };

            loop {
                if should_exit.load(Ordering::SeqCst) == true {
                    while let Some(frame) = frame_generator.try_get_next_frame()? {
                        process_frame(frame)?;
                    }
                    break;
                }
                if let Some(frame) = frame_generator.wait_for_next_frame()? {
                    process_frame(frame)?;
                } else {
                    break;
                }
            }
            Ok(())
        }
    });

    // Record
    let mut is_recording = false;
    pump_messages(|| -> Result<bool> {
        Ok(if !is_recording {
            is_recording = true;
            println!("Starting recording...");
            capture_session.start()?;
            unsafe {
                SetEvent(&start_event.0);
            }
            false
        } else {
            true
        })
    })?;
    println!("Stopping recording...");
    capture_session.stop()?;
    should_exit
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .unwrap();
    encoder_thread.join().unwrap()?;

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
