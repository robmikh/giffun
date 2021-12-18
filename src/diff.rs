use windows::{
    core::{Interface, Result},
    Graphics::SizeInt32,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
            ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_UNORDERED_ACCESS,
            D3D11_BUFFER_DESC, D3D11_BUFFER_UAV, D3D11_CPU_ACCESS_READ,
            D3D11_RESOURCE_MISC_BUFFER_STRUCTURED, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
            D3D11_UAV_DIMENSION_BUFFER, D3D11_UNORDERED_ACCESS_VIEW_DESC,
            D3D11_UNORDERED_ACCESS_VIEW_DESC_0, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        },
        Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC},
    },
};
use zerocopy::{AsBytes, FromBytes};

use crate::d3d::{read_from_buffer, Direct3D11MultiThread};

pub struct TextureDiffer {
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    multithread: Direct3D11MultiThread,
    diff_buffer: ID3D11Buffer,
    diff_default_buffer: ID3D11Buffer,
    diff_staging_buffer: ID3D11Buffer,
    previous_texture: ID3D11Texture2D,
    previous_texture_srv: ID3D11ShaderResourceView,
    first_frame: bool,
    texture_size: SizeInt32,
}

#[derive(Clone, Copy, Debug, FromBytes, AsBytes)]
#[repr(C)]
pub struct DiffRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

unsafe impl Send for TextureDiffer {}
impl TextureDiffer {
    pub fn new(
        d3d_device: &ID3D11Device,
        d3d_context: &ID3D11DeviceContext,
        texture_size: SizeInt32,
    ) -> Result<Self> {
        let previous_texture = {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: texture_size.Width as u32,
                Height: texture_size.Height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    ..Default::default()
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture2D(&desc, std::ptr::null())? }
        };
        let previous_texture_srv =
            unsafe { d3d_device.CreateShaderResourceView(&previous_texture, std::ptr::null())? };
        let diff_buffer_padded_size = (std::mem::size_of::<DiffRect>() + 3) & !0x03;
        //println!(
        //    "{} -> {}",
        //    std::mem::size_of::<DiffRect>(),
        //    diff_buffer_padded_size
        //);
        let diff_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: diff_buffer_padded_size as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS.0,
                MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0,
                StructureByteStride: diff_buffer_padded_size as u32,
                ..Default::default()
            };
            unsafe { d3d_device.CreateBuffer(&desc, std::ptr::null())? }
        };
        let diff_default_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: diff_buffer_padded_size as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0,
                ..Default::default()
            };
            let init_rect = DiffRect {
                left: texture_size.Width as u32,
                top: texture_size.Height as u32,
                right: 0,
                bottom: 0,
            };
            let mut data: Vec<u8> = init_rect.as_bytes().iter().map(|x| *x).collect();
            while data.len() < diff_buffer_padded_size {
                data.push(0);
            }
            let init_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: data.as_mut_ptr() as *mut _ as *mut _,
                ..Default::default()
            };
            unsafe { d3d_device.CreateBuffer(&desc, &init_data)? }
        };
        let diff_staging_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: diff_buffer_padded_size as u32,
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0,
                ..Default::default()
            };
            unsafe { d3d_device.CreateBuffer(&desc, std::ptr::null())? }
        };
        unsafe {
            let diff_uav = {
                let desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
                    Format: DXGI_FORMAT_UNKNOWN,
                    ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
                    Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                        Buffer: D3D11_BUFFER_UAV {
                            FirstElement: 0,
                            NumElements: 1,
                            Flags: 0,
                        },
                    },
                };
                d3d_device.CreateUnorderedAccessView(&diff_buffer, &desc)?
            };

            let diff_shader_bytes = include_bytes!["../data/generated/shaders/TextureDiff.cso"];
            let diff_shader = d3d_device.CreateComputeShader(
                diff_shader_bytes as *const _ as *const _,
                diff_shader_bytes.len(),
                None,
            )?;

            d3d_context.CSSetShader(diff_shader, std::ptr::null(), 0);
            d3d_context.CSSetUnorderedAccessViews(
                0,
                1,
                &[diff_uav] as *const _ as *const _,
                std::ptr::null(),
            );
        }
        Ok(Self {
            d3d_device: d3d_device.clone(),
            d3d_context: d3d_context.clone(),
            multithread: Direct3D11MultiThread::new(d3d_device.cast()?),
            diff_buffer,
            diff_default_buffer,
            diff_staging_buffer,
            previous_texture,
            previous_texture_srv,
            first_frame: true,
            texture_size,
        })
    }

    pub fn process_frame(&mut self, frame_texture: &ID3D11Texture2D) -> Result<Option<DiffRect>> {
        let _lock = self.multithread.lock();
        let diff_rect = if self.first_frame {
            self.first_frame = false;
            unsafe {
                self.d3d_context
                    .CopyResource(&self.previous_texture, frame_texture);
            }
            Some(DiffRect {
                left: 0,
                top: 0,
                right: self.texture_size.Width as u32,
                bottom: self.texture_size.Height as u32,
            })
        } else {
            let current_texture_srv = unsafe {
                self.d3d_device
                    .CreateShaderResourceView(frame_texture, std::ptr::null())?
            };
            let diff_rect = unsafe {
                self.d3d_context
                    .CopyResource(&self.diff_buffer, &self.diff_default_buffer);
                self.d3d_context.CSSetShaderResources(
                    0,
                    2,
                    &[current_texture_srv, self.previous_texture_srv.clone()] as *const _
                        as *const _,
                );
                self.d3d_context.Dispatch(
                    self.texture_size.Width as u32 / 2,
                    self.texture_size.Height as u32 / 2,
                    1,
                );

                self.d3d_context
                    .CopyResource(&self.diff_staging_buffer, &self.diff_buffer);
                self.d3d_context
                    .CopyResource(&self.previous_texture, frame_texture);

                let diff_rect: DiffRect =
                    read_from_buffer(&self.d3d_context, &self.diff_staging_buffer)?;
                diff_rect
            };
            if !diff_rect.is_valid() {
                None
            } else {
                Some(diff_rect)
            }
        };
        Ok(diff_rect)
    }
}

impl DiffRect {
    pub fn is_valid(&self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
    pub fn width(&self) -> u32 {
        self.right - self.left
    }
    pub fn height(&self) -> u32 {
        self.bottom - self.top
    }
}
