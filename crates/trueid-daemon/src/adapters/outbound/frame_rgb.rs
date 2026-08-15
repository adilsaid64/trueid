//! Shared `Frame` → `image::RgbImage` conversion for outbound ML adapters.
//! Not a port: core never depends on the `image` crate.

use image::{Rgb, RgbImage};
use trueid_core::{Frame, PixelFormat};

pub fn frame_to_rgb_image(frame: &Frame) -> Result<RgbImage, String> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    match frame.format {
        PixelFormat::Rgb8 => {
            let expected = w
                .checked_mul(h)
                .and_then(|n| n.checked_mul(3))
                .ok_or_else(|| "frame dimensions overflow".to_string())?;
            if frame.bytes.len() != expected {
                return Err(format!(
                    "rgb8 length {} != {}×{}×3",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                ));
            }
            RgbImage::from_raw(frame.width, frame.height, frame.bytes.clone())
                .ok_or_else(|| "invalid rgb8 buffer".to_string())
        }
        PixelFormat::Gray8 => {
            if frame.bytes.len() != w * h {
                return Err(format!(
                    "gray8 length {} != {}×{}",
                    frame.bytes.len(),
                    frame.width,
                    frame.height
                ));
            }
            let mut rgb = RgbImage::new(frame.width, frame.height);
            for y in 0..frame.height {
                for x in 0..frame.width {
                    let g = frame.bytes[(y * frame.width + x) as usize];
                    rgb.put_pixel(x, y, Rgb([g, g, g]));
                }
            }
            Ok(rgb)
        }
    }
}
