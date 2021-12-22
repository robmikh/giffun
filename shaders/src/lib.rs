pub fn lut_generation_shader() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/shaders/LUTGeneration.cso"))
}

pub fn lut_lookup_pixel_shader() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/shaders/LUTLookup_PS.cso"))
}

pub fn lut_lookup_vertex_shader() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/shaders/LUTLookup_VS.cso"))
}

pub fn texture_diff_shader() -> &'static [u8] {
    include_bytes!(concat!(env!("OUT_DIR"), "/shaders/TextureDiff.cso"))
}
