use robmikh_common::universal::d3d::get_d3d_interface_from_object;
use windows::{
    core::{Interface, Result},
    Foundation::TimeSpan,
    Graphics::{Capture::Direct3D11CaptureFrame, SizeInt32},
    Win32::Graphics::{
        Direct3D11::{
            ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11RenderTargetView,
            ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
            D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
        },
        Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
    },
};

use crate::util::d3d::Direct3D11MultiThread;

pub struct FrameCompositor {
    multithread: Direct3D11MultiThread,
    d3d_context: ID3D11DeviceContext,
    output_texture: ID3D11Texture2D,
    output_rtv: ID3D11RenderTargetView,
}

pub struct ComposedFrame<'a> {
    pub texture: &'a ID3D11Texture2D,
    pub system_relative_time: TimeSpan,
}

const CLEAR_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

unsafe impl Send for FrameCompositor {}
impl FrameCompositor {
    pub fn new(
        d3d_device: &ID3D11Device,
        d3d_context: &ID3D11DeviceContext,
        size: SizeInt32,
    ) -> Result<Self> {
        let d3d_multithread: ID3D11Multithread = d3d_device.cast()?;
        let multithread = Direct3D11MultiThread::new(d3d_multithread);

        let output_texture = {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size.Width as u32,
                Height: size.Height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    ..Default::default()
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture2D(&desc, std::ptr::null())? }
        };
        let output_rtv =
            unsafe { d3d_device.CreateRenderTargetView(&output_texture, std::ptr::null())? };

        Ok(Self {
            multithread,
            d3d_context: d3d_context.clone(),
            output_texture,
            output_rtv,
        })
    }

    pub fn process_frame<'a>(
        &'a self,
        frame: &Direct3D11CaptureFrame,
    ) -> Result<ComposedFrame<'a>> {
        let _ = self.multithread.lock();
        let frame_texture: ID3D11Texture2D = get_d3d_interface_from_object(&frame.Surface()?)?;
        let system_relative_time = frame.SystemRelativeTime()?;
        let content_size = frame.ContentSize()?;
        let desc = unsafe {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            frame_texture.GetDesc(&mut desc);
            desc
        };
        unsafe {
            self.d3d_context
                .ClearRenderTargetView(&self.output_rtv, CLEAR_COLOR.as_ptr());
        }

        // In order to support window resizing, we need to only copy out the part of
        // the buffer that contains the window. If the window is smaller than the buffer,
        // then it's a straight forward copy using the ContentSize. If the window is larger,
        // we need to clamp to the size of the buffer. For simplicity, we always clamp.
        let width = content_size.Width.clamp(0, desc.Width as i32) as u32;
        let height = content_size.Height.clamp(0, desc.Height as i32) as u32;

        let region = D3D11_BOX {
            left: 0,
            right: width,
            top: 0,
            bottom: height,
            back: 1,
            front: 0,
        };
        unsafe {
            self.d3d_context.CopySubresourceRegion(
                &self.output_texture,
                0,
                0,
                0,
                0,
                &frame_texture,
                0,
                &region,
            );
        }

        Ok(ComposedFrame {
            texture: &self.output_texture,
            system_relative_time,
        })
    }
}
