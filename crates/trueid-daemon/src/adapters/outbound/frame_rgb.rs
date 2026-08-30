//! `Frame::to_rgb_image` for outbound ML adapters.
//!
//! Extension trait — not an inherent method on [`Frame`]. That type lives in
//! core; adding `image` there would leak infrastructure into the hexagon.
//! Buffer length is already enforced by [`Frame::new`].

use image::{Rgb, RgbImage};
use trueid_core::{Frame, PixelFormat};

pub trait ToRgbImage {
    fn to_rgb_image(&self) -> Result<RgbImage, String>;
}

impl ToRgbImage for Frame {
    fn to_rgb_image(&self) -> Result<RgbImage, String> {
        match self.format() {
            PixelFormat::Rgb8 => {
                RgbImage::from_raw(self.width(), self.height(), self.bytes().to_vec())
                    .ok_or_else(|| "invalid rgb8 buffer".to_string())
            }
            PixelFormat::Gray8 => {
                let mut rgb = RgbImage::new(self.width(), self.height());
                for y in 0..self.height() {
                    for x in 0..self.width() {
                        let g = self.bytes()[(y * self.width() + x) as usize];
                        rgb.put_pixel(x, y, Rgb([g, g, g]));
                    }
                }
                Ok(rgb)
            }
        }
    }
}
