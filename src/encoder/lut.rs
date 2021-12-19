use windows::{
    core::Result,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture1D,
            ID3D11Texture3D, ID3D11UnorderedAccessView, D3D11_BIND_SHADER_RESOURCE,
            D3D11_BIND_UNORDERED_ACCESS, D3D11_TEXTURE3D_DESC, D3D11_USAGE_DEFAULT,
        },
        Dxgi::Common::DXGI_FORMAT_R8_UINT,
    },
};

pub struct PaletteIndexLUT {
    _lut_texture: ID3D11Texture3D,
    lut_shader_resource_view: ID3D11ShaderResourceView,
}

impl PaletteIndexLUT {
    pub fn new(
        d3d_device: &ID3D11Device,
        d3d_context: &ID3D11DeviceContext,
        palette_texture: &ID3D11Texture1D,
    ) -> Result<Self> {
        let lut_texture = {
            let desc = D3D11_TEXTURE3D_DESC {
                Width: 256,
                Height: 256,
                Depth: 256,
                MipLevels: 1,
                Format: DXGI_FORMAT_R8_UINT,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 | D3D11_BIND_SHADER_RESOURCE.0,
                ..Default::default()
            };
            unsafe { d3d_device.CreateTexture3D(&desc, std::ptr::null())? }
        };
        let lut_shader_resource_view =
            unsafe { d3d_device.CreateShaderResourceView(&lut_texture, std::ptr::null())? };
        let palette_shader_resource_view =
            unsafe { d3d_device.CreateShaderResourceView(palette_texture, std::ptr::null())? };
        unsafe {
            let lut_uav = { d3d_device.CreateUnorderedAccessView(&lut_texture, std::ptr::null())? };

            let lut_generation_shader_bytes =
                include_bytes!["../../data/generated/shaders/LUTGeneration.cso"];
            let lut_generation_shader = d3d_device.CreateComputeShader(
                lut_generation_shader_bytes as *const _ as *const _,
                lut_generation_shader_bytes.len(),
                None,
            )?;

            d3d_context.CSSetShader(lut_generation_shader, std::ptr::null(), 0);
            d3d_context.CSSetShaderResources(
                0,
                1,
                &[palette_shader_resource_view] as *const _ as *const _,
            );
            d3d_context.CSSetUnorderedAccessViews(
                0,
                1,
                &[lut_uav] as *const _ as *const _,
                std::ptr::null(),
            );
            d3d_context.Dispatch(256 / 8, 256 / 8, 256 / 8);

            d3d_context.CSSetShader(None, std::ptr::null(), 0);
            d3d_context.CSSetConstantBuffers(0, 0, std::ptr::null());
            let empty_uavs: [Option<ID3D11UnorderedAccessView>; 1] = [None];
            d3d_context.CSSetUnorderedAccessViews(
                0,
                1,
                &empty_uavs as *const _ as *const _,
                std::ptr::null(),
            );
        }
        Ok(Self {
            _lut_texture: lut_texture,
            lut_shader_resource_view,
        })
    }

    pub fn shader_resource_view(&self) -> ID3D11ShaderResourceView {
        self.lut_shader_resource_view.clone()
    }
}
