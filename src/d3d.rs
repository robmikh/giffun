use windows::{
    core::{Interface, Result},
    Win32::Graphics::Direct3D11::{
        ID3D11DeviceContext, ID3D11Multithread, ID3D11Resource, ID3D11Texture2D, D3D11_MAP_READ,
        D3D11_TEXTURE2D_DESC,
    },
};

pub struct Direct3D11MultiThread {
    multithread: ID3D11Multithread,
}

impl Direct3D11MultiThread {
    pub fn new(multithread: ID3D11Multithread) -> Self {
        Self { multithread }
    }

    pub fn lock<'a>(&'a self) -> Direct3D11MultithreadLock<'a> {
        Direct3D11MultithreadLock::new(&self.multithread)
    }
}

pub struct Direct3D11MultithreadLock<'a> {
    multithread: &'a ID3D11Multithread,
}

impl<'a> Direct3D11MultithreadLock<'a> {
    pub fn new(multithread: &'a ID3D11Multithread) -> Self {
        unsafe {
            multithread.Enter();
        }
        Self { multithread }
    }
}

impl<'a> Drop for Direct3D11MultithreadLock<'a> {
    fn drop(&mut self) {
        unsafe {
            self.multithread.Leave();
        }
    }
}

pub fn get_bytes_from_texture(
    d3d_context: &ID3D11DeviceContext,
    staging_texture: &ID3D11Texture2D,
    bytes_per_pixel: u32,
) -> Result<Vec<u8>> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        staging_texture.GetDesc(&mut desc as *mut _);
    }

    let resource: ID3D11Resource = staging_texture.cast()?;
    let mapped = unsafe { d3d_context.Map(Some(resource.clone()), 0, D3D11_MAP_READ, 0)? };

    // Get a slice of bytes
    let slice: &[u8] = unsafe {
        std::slice::from_raw_parts(
            mapped.pData as *const _,
            (desc.Height * mapped.RowPitch) as usize,
        )
    };

    let mut bytes = vec![0u8; (desc.Width * desc.Height * bytes_per_pixel) as usize];
    for row in 0..desc.Height {
        let data_begin = (row * (desc.Width * bytes_per_pixel)) as usize;
        let data_end = ((row + 1) * (desc.Width * bytes_per_pixel)) as usize;
        let slice_begin = (row * mapped.RowPitch) as usize;
        let slice_end = slice_begin + (desc.Width * bytes_per_pixel) as usize;
        bytes[data_begin..data_end].copy_from_slice(&slice[slice_begin..slice_end]);
    }

    unsafe { d3d_context.Unmap(Some(resource), 0) };

    Ok(bytes)
}
