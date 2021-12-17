mod hotkey;

use std::{fs::File, sync::mpsc::channel, time::Duration};

use gif::Repeat;
use robmikh_common::{desktop::{displays::get_display_handle_from_index, capture::create_capture_item_for_monitor, dispatcher_queue::DispatcherQueueControllerExtensions}, universal::{d3d::{create_d3d_device, create_direct3d_device, get_d3d_interface_from_object}}};
use windows::{core::{Result, IInspectable, Interface}, Graphics::{SizeInt32, Capture::Direct3D11CaptureFramePool, DirectX::DirectXPixelFormat}, Win32::{UI::{Input::KeyboardAndMouse::{MOD_SHIFT, MOD_CONTROL}, WindowsAndMessaging::{MSG, DispatchMessageW, WM_HOTKEY, PeekMessageW, PM_REMOVE, TranslateMessage, WM_QUIT}}, Foundation::HWND, System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED}, Graphics::{Direct3D11::{D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11_CPU_ACCESS_READ, ID3D11Texture2D, ID3D11DeviceContext, ID3D11Resource, D3D11_MAP_READ}, Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC}}}, System::{VirtualKey, DispatcherQueueController}, Foundation::TypedEventHandler};

use crate::hotkey::HotKey;

fn main() -> Result<()> {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)?; }
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
    let capture_size = SizeInt32 { Width: 100, Height: 100 };

    // Setup the gif encoder
    let mut image = File::create("target/temp.gif").unwrap();
    let mut encoder = gif::Encoder::new(&mut image, capture_size.Width as u16, capture_size.Height as u16, &[]).unwrap();
    encoder.set_repeat(Repeat::Infinite).unwrap();

    // Create a staging texture that is the same size as our capture
    let staging_texture = {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: capture_size.Width as u32,
            Height: capture_size.Height as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                ..Default::default()
            },
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ,
            ..Default::default()
        };
        unsafe {
            d3d_device.CreateTexture2D(&desc, std::ptr::null())?
        }
    };

    // Setup capture
    let frame_pool = Direct3D11CaptureFramePool::Create(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 20, &capture_size)?;
    let capture_session = frame_pool.CreateCaptureSession(&capture_item)?;
    let (sender, receiver) = channel();
    frame_pool.FrameArrived(TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new( {
        move |frame_pool, _| {
            let frame_pool = frame_pool.as_ref().unwrap();
            let frame = frame_pool.TryGetNextFrame()?;
            sender.send(frame).unwrap();
            Ok(())
        }
    }))?;
    
    // Record
    let mut is_recording = false;
    let mut last_timestamp = None;
    pump_messages(|| -> Result<bool> {
        Ok(if !is_recording {
            is_recording = true;
            println!("Starting recording...");
            capture_session.StartCapture()?;
            false
        } else {
            true
        })
    },
    || -> Result<()> {
        for frame in receiver.try_iter() {
            let frame_texture: ID3D11Texture2D =
                get_d3d_interface_from_object(&frame.Surface()?)?;

            // Copy the frame texture to our staging texture and then copy the bits.
            unsafe {
                d3d_context.CopyResource(Some(staging_texture.cast()?), Some(frame_texture.cast()?));
            }
            let mut gif_frame = None;
            ref_bytes_from_texture(&d3d_context, &staging_texture, |bytes, width, height, stride| -> Result<()> {
                gif_frame = Some(gif::Frame::from_bgra_with_stride_speed(width as u16, height as u16, bytes, stride as usize, 10));
                Ok(())
            })?;

            // Build our gif frame
            let mut gif_frame = gif_frame.unwrap();
            let timestamp: Duration = if last_timestamp.is_none() {
                let timestamp = frame.SystemRelativeTime()?;
                timestamp
            } else {
                last_timestamp.unwrap()
            }.into();
            let current_timestamp: Duration = {
                let current_timestamp = frame.SystemRelativeTime()?;
                last_timestamp = Some(current_timestamp);
                current_timestamp
            }.into();
            let frame_delay = current_timestamp - timestamp;
            //println!("delay: {}", frame_delay.as_millis());
            gif_frame.delay = (frame_delay.as_millis() / 10) as u16;

            // Write our frame to disk
            encoder.write_frame(&gif_frame).unwrap();
        }
        Ok(())
    })?;
    println!("Stopping recording...");
    frame_pool.Close()?;
    capture_session.Close()?;

    Ok(())
}


fn pump_messages<F: FnMut() -> Result<bool>, G: FnMut() -> Result<()>>(mut hot_key_callback: F, mut message_batch_ended_callback: G) -> Result<()> {
    let _hot_key = HotKey::new(MOD_SHIFT | MOD_CONTROL, VirtualKey::R.0 as u32)?;
    println!("Press SHIFT+CTRL+R to start/stop the recording...");
    let mut message = MSG::default();
    'main_loop : loop {
        unsafe {
            while PeekMessageW(&mut message, HWND(0), 0, 0, PM_REMOVE).into() {
                match message.message {
                    //WM_QUIT => break,
                    WM_HOTKEY => {
                        if hot_key_callback()? {
                            break 'main_loop;
                        }
                    }
                    _ => {}
                }
                TranslateMessage(&mut message);
                DispatchMessageW(&mut message);
            }
        }

        message_batch_ended_callback()?;
    }
    Ok(())
}

fn ref_bytes_from_texture<F: FnOnce(&[u8], u32, u32, u32) -> Result<()>>(d3d_context: &ID3D11DeviceContext, staging_texture: &ID3D11Texture2D, mut bytes_callback: F) -> Result<()> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { staging_texture.GetDesc(&mut desc as *mut _); }

    let resource: ID3D11Resource = staging_texture.cast()?;
    let mapped = unsafe { d3d_context.Map(Some(resource.clone()), 0, D3D11_MAP_READ, 0)? };

    // Get a slice of bytes
    let slice: &[u8] = unsafe {
        std::slice::from_raw_parts(
            mapped.pData as *const _,
            (desc.Height * mapped.RowPitch) as usize,
        )
    };

    let result = bytes_callback(slice, desc.Width, desc.Height, mapped.RowPitch);
    unsafe { d3d_context.Unmap(Some(resource), 0) };
    result?;

    Ok(())
}