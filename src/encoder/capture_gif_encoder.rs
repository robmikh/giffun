use std::{
    borrow::Cow,
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use gif::{Frame, Repeat};
use robmikh_common::universal::d3d::{create_direct3d_device, get_d3d_interface_from_object};
use windows::{
    core::{Handle, Result},
    Graphics::{
        Capture::{Direct3D11CaptureFrame, GraphicsCaptureItem},
        SizeInt32,
    },
    Win32::{
        Foundation::PWSTR,
        Graphics::{
            Direct3D11::{
                ID3D11Device, ID3D11DeviceContext, ID3D11Texture1D, ID3D11Texture2D,
                D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE1D_DESC,
                D3D11_USAGE_DEFAULT,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UINT,
        },
        System::Threading::{CreateEventW, SetEvent, WaitForSingleObject, WAIT_OBJECT_0},
    },
};

use crate::{
    capture::frame_generator::{CaptureFrameGenerator, CaptureFrameGeneratorSession},
    util::handle::AutoCloseHandle,
};

use super::{diff::TextureDiffer, lut::PaletteIndexLUT, quantizer::ColorQuantizer};

pub struct CaptureGifEncoder {
    _d3d_device: ID3D11Device,
    _d3d_context: ID3D11DeviceContext,
    _palette_texture: ID3D11Texture1D,
    capture_session: CaptureFrameGeneratorSession,
    should_exit: Arc<AtomicBool>,
    start_event: AutoCloseHandle,
    encoder_thread: JoinHandle<Result<()>>,
    started: AtomicBool,
}

const INFINITE: u32 = 0xFFFFFFFF;

impl CaptureGifEncoder {
    pub fn new<P: AsRef<Path>>(
        d3d_device: &ID3D11Device,
        palette: &[u8],
        capture_item: GraphicsCaptureItem,
        capture_size: SizeInt32,
        path: P,
    ) -> Result<Self> {
        let capture_size = ensure_even_size(capture_size);

        let d3d_context = unsafe {
            let mut d3d_context = None;
            d3d_device.GetImmediateContext(&mut d3d_context);
            d3d_context.unwrap()
        };
        let device = create_direct3d_device(d3d_device)?;

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

        // Create our differ
        let mut differ = TextureDiffer::new(&d3d_device, &d3d_context, capture_size)?;

        // Setup capture
        let mut frame_generator =
            CaptureFrameGenerator::new(device, capture_item, capture_size, 2)?;
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
            let path = path.as_ref().to_owned();
            let palette: Vec<u8> = palette.iter().map(|x| *x).collect();
            move || -> Result<()> {
                assert!(unsafe { WaitForSingleObject(start_event, INFINITE) } == WAIT_OBJECT_0);

                // Setup the gif encoder
                let mut image = File::create(path).unwrap();
                let mut encoder = gif::Encoder::new(
                    &mut image,
                    capture_size.Width as u16,
                    capture_size.Height as u16,
                    &palette,
                )
                .unwrap();
                encoder.set_repeat(Repeat::Infinite).unwrap();

                let mut last_timestamp = None;
                let mut process_frame = |frame: Direct3D11CaptureFrame| -> Result<()> {
                    let (bytes, rect) = {
                        let frame_texture: ID3D11Texture2D =
                            get_d3d_interface_from_object(&frame.Surface()?)?;
                        let bytes = quantizer.quantize(&frame_texture)?;

                        let rect = differ.process_frame(&frame_texture)?;
                        //println!("{:?}", rect);

                        (bytes, rect)
                    };

                    // If there's no change, don't bother
                    if let Some(mut rect) = rect {
                        // Inflate our rect to eliminate artifacts
                        let inflate_amount = 1;
                        let left = rect.left as i32 - inflate_amount;
                        let top = rect.top as i32 - inflate_amount;
                        let right = rect.right as i32 + inflate_amount;
                        let bottom = rect.bottom as i32 + inflate_amount;
                        //println!("{:?}", rect);
                        rect.left = left.max(0) as u32;
                        rect.top = top.max(0) as u32;
                        rect.right = right.min(capture_size.Width) as u32;
                        rect.bottom = bottom.min(capture_size.Height) as u32;
                        //println!("{:?}", rect);
                        //println!("");

                        // Build our gif frame
                        let width = rect.width();
                        let height = rect.height();
                        let bytes = if width == capture_size.Width as u32
                            && height == capture_size.Height as u32
                        {
                            bytes
                        } else {
                            let mut new_bytes = vec![0u8; (width * height) as usize];
                            for j in 0..height {
                                let dest_start = (j * width) as usize;
                                let dest_end = dest_start + width as usize;
                                let src_start = (((j + rect.top) * capture_size.Width as u32)
                                    + rect.left)
                                    as usize;
                                let src_end = src_start + width as usize;
                                new_bytes[dest_start..dest_end]
                                    .copy_from_slice(&bytes[src_start..src_end]);
                            }
                            new_bytes
                        };
                        //println!("{} x {} ({}) vs {}", width, height, width * height, bytes.len());
                        let mut gif_frame =
                            create_gif_frame(width as u16, height as u16, &bytes, None);
                        gif_frame.left = rect.left as u16;
                        gif_frame.top = rect.top as u16;
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
                    }

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
        Ok(Self {
            _d3d_device: d3d_device.clone(),
            _d3d_context: d3d_context,
            _palette_texture: palette_texture,
            capture_session,
            should_exit,
            start_event,
            encoder_thread,
            started: AtomicBool::new(false),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        if self
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.capture_session.start()?;
            unsafe {
                SetEvent(&self.start_event.0);
            }
        }
        Ok(())
    }

    pub fn stop(self) -> Result<()> {
        self.capture_session.stop()?;
        self.should_exit
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .unwrap();
        self.encoder_thread.join().unwrap()?;
        Ok(())
    }
}

fn create_gif_frame<'a>(
    width: u16,
    height: u16,
    pixels: &'a [u8],
    transparent: Option<u8>,
) -> Frame<'a> {
    assert_eq!(
        width as usize * height as usize,
        pixels.len(),
        "Too many or too little pixels for the given width and height to create a GIF Frame"
    );

    Frame {
        width,
        height,
        buffer: Cow::Borrowed(pixels),
        palette: None,
        transparent,
        ..Frame::default()
    }
}

fn ensure_even(value: i32) -> i32 {
    if value % 2 == 0 {
        value
    } else {
        value + 1
    }
}

fn ensure_even_size(size: SizeInt32) -> SizeInt32 {
    SizeInt32 {
        Width: ensure_even(size.Width),
        Height: ensure_even(size.Height),
    }
}
