use windows::{
    core::{Interface, Result},
    Graphics::RectInt32,
    Win32::Graphics::Direct3D11::{
        ID3D11Buffer, ID3D11DeviceContext, ID3D11Multithread, ID3D11Resource, ID3D11Texture2D,
        D3D11_BUFFER_DESC, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC,
    },
};
use zerocopy::FromBytes;

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
    rect: RectInt32,
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

    let mut bytes = vec![0u8; (rect.Width * rect.Height * bytes_per_pixel as i32) as usize];
    for row in 0..rect.Height {
        let data_begin = (row * (rect.Width * bytes_per_pixel as i32)) as usize;
        let data_end = data_begin + (rect.Width * bytes_per_pixel as i32) as usize;
        let slice_begin = (((row + rect.Y) * mapped.RowPitch as i32)
            + (rect.X * bytes_per_pixel as i32)) as usize;
        let slice_end = slice_begin + (rect.Width * bytes_per_pixel as i32) as usize;
        bytes[data_begin..data_end].copy_from_slice(&slice[slice_begin..slice_end]);
    }

    unsafe { d3d_context.Unmap(Some(resource), 0) };

    Ok(bytes)
}

pub fn read_from_buffer<T: FromBytes>(
    d3d_context: &ID3D11DeviceContext,
    staging_buffer: &ID3D11Buffer,
) -> Result<T> {
    let mut desc = D3D11_BUFFER_DESC::default();
    unsafe {
        staging_buffer.GetDesc(&mut desc as *mut _);
    }

    assert!(std::mem::size_of::<T>() <= desc.ByteWidth as usize);

    let resource: ID3D11Resource = staging_buffer.cast()?;
    let mapped = unsafe { d3d_context.Map(Some(resource.clone()), 0, D3D11_MAP_READ, 0)? };

    // Get a slice of bytes
    let slice: &[u8] =
        unsafe { std::slice::from_raw_parts(mapped.pData as *const _, std::mem::size_of::<T>()) };

    let result = T::read_from(slice).unwrap();

    unsafe { d3d_context.Unmap(Some(resource), 0) };

    Ok(result)
}
