use windows::{
    core::{Interface, Result},
    Foundation::Numerics::{Vector2, Vector3},
    Graphics::SizeInt32,
    Win32::{
        Foundation::PSTR,
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::{
                ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
                ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
                D3D11_BIND_INDEX_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
                D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER,
                D3D11_CPU_ACCESS_READ, D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_INPUT_ELEMENT_DESC,
                D3D11_INPUT_PER_VERTEX_DATA, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA,
                D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_WRAP, D3D11_USAGE_DEFAULT,
                D3D11_USAGE_STAGING, D3D11_VIEWPORT,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32G32B32_FLOAT,
                DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8_UINT, DXGI_SAMPLE_DESC,
            },
        },
    },
};

use crate::util::d3d::{get_bytes_from_texture, Direct3D11MultiThread};

use super::lut::PaletteIndexLUT;

pub struct ColorQuantizer {
    input_texture: ID3D11Texture2D,
    _input_shader_resource_view: ID3D11ShaderResourceView,
    _input_sampler: ID3D11SamplerState,
    output_texture: ID3D11Texture2D,
    _output_texture_render_target_view: ID3D11RenderTargetView,
    _vertex_buffer: ID3D11Buffer,
    _index_buffer: ID3D11Buffer,
    staging_texture: ID3D11Texture2D,
    d3d_context: ID3D11DeviceContext,
    multithread: Direct3D11MultiThread,
    _lut: PaletteIndexLUT,
}

unsafe impl Send for ColorQuantizer {}
impl ColorQuantizer {
    pub fn new(
        d3d_device: &ID3D11Device,
        d3d_context: &ID3D11DeviceContext,
        lut: PaletteIndexLUT,
        capture_size: SizeInt32,
    ) -> Result<Self> {
        // Create a texture as the input to the lookup shader
        let input_texture = {
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
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture2D(&desc, std::ptr::null())? }
        };
        let input_shader_resource_view =
            unsafe { d3d_device.CreateShaderResourceView(&input_texture, std::ptr::null())? };
        let input_sampler = {
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                ..Default::default()
            };
            unsafe { d3d_device.CreateSamplerState(&desc)? }
        };

        // Create a texture for our palettized output
        let output_texture = {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: capture_size.Width as u32,
                Height: capture_size.Height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8_UINT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    ..Default::default()
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture2D(&desc, std::ptr::null())? }
        };
        let output_texture_render_target_view =
            unsafe { d3d_device.CreateRenderTargetView(&output_texture, std::ptr::null())? };
        // Define quad vertices/indices
        let (vertex_buffer, index_buffer) = {
            let mut vertex_data = vec![
                Vertex::new(Vector3::new(-1.0, -1.0, 0.0), Vector2::new(0.0, 1.0)),
                Vertex::new(Vector3::new(-1.0, 1.0, 0.0), Vector2::new(0.0, 0.0)),
                Vertex::new(Vector3::new(1.0, 1.0, 0.0), Vector2::new(1.0, 0.0)),
                Vertex::new(Vector3::new(1.0, -1.0, 0.0), Vector2::new(1.0, 1.0)),
            ];
            let mut index_data = vec![0u16, 1, 2, 3, 0, 2];

            let vertex_buffer = {
                let desc = D3D11_BUFFER_DESC {
                    ByteWidth: (vertex_data.len() * std::mem::size_of::<Vertex>()) as u32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0,
                    ..Default::default()
                };
                // TODO: pSysMem shouldn't be *mut _
                let subresource_data = D3D11_SUBRESOURCE_DATA {
                    pSysMem: vertex_data.as_mut_ptr() as *mut _ as *mut _,
                    ..Default::default()
                };
                unsafe { d3d_device.CreateBuffer(&desc, &subresource_data)? }
            };
            let index_buffer = {
                let desc = D3D11_BUFFER_DESC {
                    ByteWidth: (index_data.len() * std::mem::size_of::<u16>()) as u32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_INDEX_BUFFER.0,
                    ..Default::default()
                };
                // TODO: pSysMem shouldn't be *mut _
                let subresource_data = D3D11_SUBRESOURCE_DATA {
                    pSysMem: index_data.as_mut_ptr() as *mut _ as *mut _,
                    ..Default::default()
                };
                unsafe { d3d_device.CreateBuffer(&desc, &subresource_data)? }
            };

            (vertex_buffer, index_buffer)
        };
        unsafe {
            // Load LUT lookup shaders
            let lut_lookup_pixel_shader_bytes =
                include_bytes!["../../data/generated/shaders/LUTLookup_PS.cso"];
            let lut_lookup_pixel_shader = d3d_device.CreatePixelShader(
                lut_lookup_pixel_shader_bytes as *const _ as *const _,
                lut_lookup_pixel_shader_bytes.len(),
                None,
            )?;
            let lut_lookup_vertex_shader_bytes =
                include_bytes!["../../data/generated/shaders/LUTLookup_VS.cso"];
            let lut_lookup_vertex_shader = d3d_device.CreateVertexShader(
                lut_lookup_vertex_shader_bytes as *const _ as *const _,
                lut_lookup_vertex_shader_bytes.len(),
                None,
            )?;
            d3d_context.VSSetShader(lut_lookup_vertex_shader, std::ptr::null(), 0);
            d3d_context.PSSetShader(lut_lookup_pixel_shader, std::ptr::null(), 0);

            // Create our vertex input layout
            let mut position_name: Vec<u8> = b"POSITION\0".iter().map(|x| *x).collect();
            let mut texcoord_name: Vec<u8> = b"TEXCOORD\0".iter().map(|x| *x).collect();
            let input_layout_data = [
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PSTR(position_name.as_mut_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PSTR(texcoord_name.as_mut_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 12,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];
            let input_layout = d3d_device.CreateInputLayout(
                &input_layout_data as *const _ as *const _,
                input_layout_data.len() as u32,
                lut_lookup_vertex_shader_bytes as *const _ as *const _,
                lut_lookup_vertex_shader_bytes.len(),
            )?;
            d3d_context.IASetInputLayout(input_layout);
            d3d_context.IASetVertexBuffers(
                0,
                1,
                &[Some(vertex_buffer.clone())] as *const _ as *const _,
                &[std::mem::size_of::<Vertex>() as u32] as *const _ as *const _,
                &[0u32] as *const _ as *const _,
            );
            d3d_context.IASetIndexBuffer(&index_buffer, DXGI_FORMAT_R16_UINT, 0);
            d3d_context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            d3d_context.RSSetViewports(
                1,
                &[D3D11_VIEWPORT {
                    TopLeftX: 0.0,
                    TopLeftY: 0.0,
                    Width: capture_size.Width as f32,
                    Height: capture_size.Height as f32,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                }] as *const _ as *const _,
            );

            d3d_context.PSSetSamplers(0, 1, &[input_sampler.clone()] as *const _ as *const _);
            d3d_context.PSSetShaderResources(
                0,
                2,
                &[
                    input_shader_resource_view.clone(),
                    lut.shader_resource_view(),
                ] as *const _ as *const _,
            );
            d3d_context.OMSetRenderTargets(
                1,
                &[output_texture_render_target_view.clone()] as *const _ as *const _,
                None,
            )
        }

        // Create a staging texture that matches our output texture
        let staging_texture = {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: capture_size.Width as u32,
                Height: capture_size.Height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8_UINT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    ..Default::default()
                },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture2D(&desc, std::ptr::null())? }
        };

        Ok(Self {
            input_texture,
            _input_shader_resource_view: input_shader_resource_view,
            _input_sampler: input_sampler,
            output_texture,
            _output_texture_render_target_view: output_texture_render_target_view,
            _vertex_buffer: vertex_buffer,
            _index_buffer: index_buffer,
            staging_texture,
            d3d_context: d3d_context.clone(),
            multithread: Direct3D11MultiThread::new(d3d_device.cast()?),
            _lut: lut,
        })
    }

    pub fn quantize(&self, frame_texture: &ID3D11Texture2D) -> Result<Vec<u8>> {
        let bytes = {
            let _lock = self.multithread.lock();

            // Copy our frame texture to the input texture of our pipeline
            unsafe {
                self.d3d_context
                    .CopyResource(&self.input_texture, frame_texture);
            }

            // Run the input texture through the LUT
            unsafe {
                self.d3d_context.DrawIndexed(6, 0, 0);
            }

            // Copy the output texture to our staging texture and then copy the bits.
            unsafe {
                self.d3d_context.CopyResource(
                    Some(self.staging_texture.cast()?),
                    Some(self.output_texture.cast()?),
                );
            }
            let bytes = get_bytes_from_texture(&self.d3d_context, &self.staging_texture, 1)?;
            bytes
        };
        Ok(bytes)
    }
}

#[repr(C)]
struct Vertex {
    position: Vector3,
    texcoord: Vector2,
}

impl Vertex {
    pub fn new(position: Vector3, texcoord: Vector2) -> Self {
        Self { position, texcoord }
    }
}
