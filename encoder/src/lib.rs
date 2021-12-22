extern crate gif;
extern crate robmikh_common;
extern crate windows;
extern crate zerocopy;

mod capture;
mod encoder;
mod util;

pub use encoder::capture_gif_encoder::CaptureGifEncoder;
pub use encoder::palette::DEFAULT_PALETTE;
