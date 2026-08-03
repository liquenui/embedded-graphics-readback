//! [`ReadbackTarget`] for [`embedded_graphics::framebuffer::Framebuffer`].
//!
//! Enabled by the `framebuffer` feature. `Framebuffer` keeps its pixels in a
//! `[u8; N]` and already implements [`GetPixel`], so it satisfies the whole
//! contract: reads are slice arithmetic, and
//! [`read_area`](ReadbackTarget::read_area) builds the backing image once per
//! region rather than once per pixel.
//!
//! The impl covers every `Framebuffer` that is both drawable and readable —
//! every colour type and data order for which `embedded-graphics` provides
//! those impls — so enabling the feature is all that is required:
//!
//! ```
//! use embedded_graphics::{
//!     framebuffer::{buffer_size, Framebuffer},
//!     pixelcolor::{raw::LittleEndian, Rgb565},
//!     prelude::*,
//!     primitives::PrimitiveStyle,
//! };
//! use embedded_graphics_readback::ReadbackTarget;
//!
//! let mut fb =
//!     Framebuffer::<Rgb565, _, LittleEndian, 64, 64, { buffer_size::<Rgb565>(64, 64) }>::new();
//!
//! fb.bounding_box()
//!     .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
//!     .draw(&mut fb)?;
//!
//! assert_eq!(fb.read_pixel(Point::new(10, 10)), Some(Rgb565::RED));
//! # Ok::<(), core::convert::Infallible>(())
//! ```

use embedded_graphics::{
    draw_target::DrawTarget,
    framebuffer::Framebuffer,
    geometry::Point,
    image::GetPixel,
    iterator::raw::RawDataSlice,
    pixelcolor::{PixelColor, raw::ByteOrder},
    primitives::{PointsIter, Rectangle},
};

use crate::ReadbackTarget;

impl<C, BO, const WIDTH: usize, const HEIGHT: usize, const N: usize> ReadbackTarget
    for Framebuffer<C, C::Raw, BO, WIDTH, HEIGHT, N>
where
    Self: DrawTarget<Color = C>,
    C: PixelColor + From<C::Raw>,
    BO: ByteOrder,
    for<'a> RawDataSlice<'a, C::Raw, BO>: IntoIterator<Item = C::Raw>,
{
    fn read_pixel(&self, point: Point) -> Option<C> {
        GetPixel::pixel(self, point)
    }

    fn read_area(&self, area: &Rectangle, out: &mut [C]) -> usize {
        // Hoist the backing image out of the loop: one construction per region
        // instead of one per pixel.
        let image = self.as_image();
        let mut written = 0;
        for (slot, point) in out.iter_mut().zip(area.points()) {
            if let Some(color) = image.pixel(point) {
                *slot = color;
                written += 1;
            }
        }
        written
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::{
        Pixel,
        framebuffer::buffer_size,
        geometry::Size,
        pixelcolor::{
            Rgb565,
            raw::{LittleEndian, RawU16},
        },
        prelude::*,
    };

    type Fb = Framebuffer<Rgb565, RawU16, LittleEndian, 4, 4, { buffer_size::<Rgb565>(4, 4) }>;

    #[test]
    fn read_pixel_round_trips_a_write() {
        let mut fb = Fb::new();
        Pixel(Point::new(1, 2), Rgb565::RED).draw(&mut fb).unwrap();

        assert_eq!(fb.read_pixel(Point::new(1, 2)), Some(Rgb565::RED));
        assert_eq!(fb.read_pixel(Point::new(0, 0)), Some(Rgb565::BLACK));
    }

    #[test]
    fn read_pixel_out_of_bounds_is_none() {
        let fb = Fb::new();

        assert_eq!(fb.read_pixel(Point::new(-1, 0)), None);
        assert_eq!(fb.read_pixel(Point::new(4, 0)), None);
        assert_eq!(fb.read_pixel(Point::new(0, 4)), None);
    }

    #[test]
    fn read_area_override_matches_read_pixel() {
        let mut fb = Fb::new();
        Pixel(Point::new(0, 0), Rgb565::RED).draw(&mut fb).unwrap();
        Pixel(Point::new(1, 0), Rgb565::GREEN)
            .draw(&mut fb)
            .unwrap();
        Pixel(Point::new(0, 1), Rgb565::BLUE).draw(&mut fb).unwrap();

        let area = Rectangle::new(Point::zero(), Size::new(2, 2));
        let mut out = [Rgb565::WHITE; 4];
        let n = fb.read_area(&area, &mut out);

        assert_eq!(n, 4);
        assert_eq!(
            out,
            [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::BLACK]
        );

        // The override must agree with the trait's per-pixel default.
        let by_pixel: [Rgb565; 4] = core::array::from_fn(|i| {
            fb.read_pixel(Point::new(i as i32 % 2, i as i32 / 2))
                .unwrap()
        });
        assert_eq!(out, by_pixel);
    }

    #[test]
    fn read_area_counts_only_in_bounds() {
        let fb = Fb::new();
        // Straddling the right edge: only the in-bounds column counts.
        let area = Rectangle::new(Point::new(3, 0), Size::new(2, 2));
        let mut out = [Rgb565::WHITE; 4];

        assert_eq!(fb.read_area(&area, &mut out), 2);
    }
}
