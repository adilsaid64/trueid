//! `Frame::to_rgb_image` for outbound ML adapters.
//!
//! Extension trait — not an inherent method on [`Frame`]. That type lives in
//! core; adding `image` there would leak infrastructure into the hexagon.

use image::{Rgb, RgbImage};
use trueid_core::{Frame, PixelFormat};

pub trait ToRgbImage {
    fn to_rgb_image(&self) -> Result<RgbImage, String>;
}

impl ToRgbImage for Frame {
    fn to_rgb_image(&self) -> Result<RgbImage, String> {
        let w = self.width as usize;
        let h = self.height as usize;
        match self.format {
            PixelFormat::Rgb8 => {
                let expected = w
                    .checked_mul(h)
                    .and_then(|n| n.checked_mul(3))
                    .ok_or_else(|| "frame dimensions overflow".to_string())?;
                if self.bytes.len() != expected {
                    return Err(format!(
                        "rgb8 length {} != {}×{}×3",
                        self.bytes.len(),
                        self.width,
                        self.height
                    ));
                }
                RgbImage::from_raw(self.width, self.height, self.bytes.clone())
                    .ok_or_else(|| "invalid rgb8 buffer".to_string())
            }
            PixelFormat::Gray8 => {
                if self.bytes.len() != w * h {
                    return Err(format!(
                        "gray8 length {} != {}×{}",
                        self.bytes.len(),
                        self.width,
                        self.height
                    ));
                }
                let mut rgb = RgbImage::new(self.width, self.height);
                for y in 0..self.height {
                    for x in 0..self.width {
                        let g = self.bytes[(y * self.width + x) as usize];
                        rgb.put_pixel(x, y, Rgb([g, g, g]));
                    }
                }
                Ok(rgb)
            }
        }
    }
}
