#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
//! A pixel-readback capability for [`embedded-graphics`] draw targets.
//!
//! Compositing needs to know what is already on the target before it writes.
//! Antialiased edges, translucent fills, blend modes and every other
//! read-modify-write effect blend a foreground colour *into* a backdrop, and
//! the backdrop is whatever the target currently holds. [`ReadbackTarget`] is
//! the capability that says a target can answer that question, so rendering
//! code can require it generically:
//!
//! ```rust,ignore
//! /// Blend `fg` over one scanline run at the given per-pixel coverage.
//! fn composite_run<D: ReadbackTarget>(
//!     target: &mut D,
//!     run: &Rectangle,
//!     fg: D::Color,
//!     coverage: &[u8],
//!     scratch: &mut [D::Color],
//! ) {
//!     target.read_area(run, scratch);
//!     for (px, &cov) in scratch.iter_mut().zip(coverage) {
//!         *px = blend_over(fg, *px, cov);
//!     }
//!     let _ = target.fill_contiguous(run, scratch.iter().copied());
//! }
//! ```
//!
//! `ReadbackTarget: DrawTarget`, so a single bound supplies both halves of that
//! loop and a single `Self::Color` names the pixel type throughout. The
//! immutable read finishes before the mutable write begins, so one `&mut D`
//! carries the whole operation — no staging buffer for the shape's bounding
//! box, just a scratch run.
//!
//! It is a thin, dependency-light contract over [`embedded-graphics-core`]:
//! implement [`read_pixel`](ReadbackTarget::read_pixel) and the rest follows.
//! [`read_area`](ReadbackTarget::read_area) copies a region out row-major in
//! one call, defaulting to a `read_pixel` loop and overridable with a block
//! copy.
//!
//! Because the capability belongs to the *target*, wrappers forward it:
//! clipping, translating and offscreen adapters stay readback-capable by
//! delegating both methods, so a pipeline keeps its capability all the way
//! down.
//!
//! # Delegating from `GetPixel`
//!
//! [`read_pixel`](ReadbackTarget::read_pixel) shares its signature with
//! [`GetPixel::pixel`](embedded_graphics_core::image::GetPixel::pixel), so a
//! target that already implements that trait delegates in one line:
//!
//! ```
//! use embedded_graphics_core::{draw_target::DrawTarget, geometry::Point, image::GetPixel};
//! use embedded_graphics_readback::ReadbackTarget;
//!
//! # struct Fb;
//! # impl GetPixel for Fb {
//! #     type Color = embedded_graphics_core::pixelcolor::Rgb565;
//! #     fn pixel(&self, _p: Point) -> Option<Self::Color> { None }
//! # }
//! # impl embedded_graphics_core::geometry::Dimensions for Fb {
//! #     fn bounding_box(&self) -> embedded_graphics_core::primitives::Rectangle { Default::default() }
//! # }
//! # impl DrawTarget for Fb {
//! #     type Color = embedded_graphics_core::pixelcolor::Rgb565;
//! #     type Error = core::convert::Infallible;
//! #     fn draw_iter<I>(&mut self, _: I) -> Result<(), Self::Error>
//! #     where I: IntoIterator<Item = embedded_graphics_core::Pixel<Self::Color>> { Ok(()) }
//! # }
//! impl ReadbackTarget for Fb {
//!     fn read_pixel(&self, point: Point) -> Option<Self::Color> {
//!         GetPixel::pixel(self, point)
//!     }
//! }
//! ```
//!
//! A reader that returns a bare colour needs a bounds guard, since `read_pixel`
//! answers `None` outside the bounding box:
//!
//! ```rust,ignore
//! fn read_pixel(&self, p: Point) -> Option<Rgb888> {
//!     self.bounding_box().contains(p).then(|| self.0.get_pixel(p))
//! }
//! ```
//!
//! Readback is opt-in rather than blanket-implemented, so it stays a deliberate
//! statement about a target and can carry the cost contract on
//! [`ReadbackTarget`].
//!
//! # `embedded-graphics` framebuffers
//!
//! [`embedded_graphics::framebuffer::Framebuffer`] is supported out of the box
//! under the `framebuffer` feature:
//!
//! ```toml
//! embedded-graphics-readback = { version = "0.1", features = ["framebuffer"] }
//! ```
//!
//! The impl covers every colour type and data order `embedded-graphics` makes
//! drawable, and overrides [`read_area`](ReadbackTarget::read_area) to build
//! the backing image once per region. Draw into a `Framebuffer`, hand it to any
//! `ReadbackTarget` renderer, and flush it to the panel.
//!
//! `Framebuffer` is also the shortest route to readback for a streaming driver:
//! render into one, then push it out over the bus.
//!
//! # Who implements it
//!
//! Framebuffers and canvases — anything holding pixels in RAM, where a read is
//! a slice index — are the natural implementors, along with buffered display
//! drivers that expose their buffer and simulators wrapped in a newtype.
//! Streaming drivers hold no pixels, so they leave the trait unimplemented and
//! callers keep the write-only path.
//!
//! [`embedded_graphics::framebuffer::Framebuffer`]:
//!     https://docs.rs/embedded-graphics/latest/embedded_graphics/framebuffer/struct.Framebuffer.html
//!
//! [`embedded-graphics`]: https://docs.rs/embedded-graphics
//! [`embedded-graphics-core`]: https://docs.rs/embedded-graphics-core
//! [`DrawTarget`]: embedded_graphics_core::draw_target::DrawTarget

#[cfg(feature = "framebuffer")]
#[cfg_attr(docsrs, doc(cfg(feature = "framebuffer")))]
mod framebuffer;

use embedded_graphics_core::{
    draw_target::DrawTarget,
    geometry::Point,
    primitives::{PointsIter, Rectangle},
};

/// A [`DrawTarget`] that can read back the pixels it has drawn.
///
/// Implement this for targets that retain their pixels in memory (framebuffers,
/// canvases, simulators). Streaming targets that forward straight to a bus
/// cannot read back and should not implement it.
///
/// # Cost contract
///
/// Destination-aware renderers call [`read_area`](Self::read_area) **once per
/// run on the hot path** — they assume a read is roughly the cost of a write.
/// That holds for RAM-backed framebuffers (a slice copy) and DMA-capable
/// panels, which are the intended targets. A panel whose only read primitive is
/// a slow per-pixel bus command must **not** implement this trait merely
/// because it can: leaving the default [`read_area`](Self::read_area) in place
/// turns every antialiased run into `width` bus round-trips and makes
/// read-modify-write rendering O(pixels) on the bus. Implement it only when
/// reads are cheap, and override [`read_area`](Self::read_area) with a block
/// copy whenever the framebuffer allows it.
pub trait ReadbackTarget: DrawTarget {
    /// The colour currently at `point`, or `None` if `point` lies outside the
    /// target's [bounding box](embedded_graphics_core::geometry::Dimensions::bounding_box).
    ///
    /// Matches [`GetPixel::pixel`](embedded_graphics_core::image::GetPixel::pixel).
    fn read_pixel(&self, point: Point) -> Option<Self::Color>;

    /// Read the pixels of `area` row-major into `out`, returning how many were
    /// in bounds (and so written).
    ///
    /// The default reads pixel-by-pixel via [`read_pixel`](Self::read_pixel):
    /// each in-bounds colour is written to the matching `out` slot and
    /// out-of-bounds slots are left untouched. Iteration stops at whichever of
    /// `area` or `out` is exhausted first. Override this when the target can
    /// copy a whole region out of its framebuffer in one operation.
    fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
        let mut written = 0;
        for (slot, point) in out.iter_mut().zip(area.points()) {
            if let Some(color) = self.read_pixel(point) {
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
    use embedded_graphics::prelude::*;
    use embedded_graphics_core::{pixelcolor::Rgb565, primitives::Rectangle};

    /// A tiny 4×4 RGB565 framebuffer for exercising the trait.
    struct TestFb {
        pixels: [Rgb565; 16],
    }

    impl TestFb {
        fn new() -> Self {
            Self {
                pixels: [Rgb565::BLACK; 16],
            }
        }
        fn index(p: Point) -> Option<usize> {
            (0..4).contains(&p.x).then_some(())?;
            (0..4).contains(&p.y).then_some(())?;
            Some((p.y * 4 + p.x) as usize)
        }
    }

    impl OriginDimensions for TestFb {
        fn size(&self) -> Size {
            Size::new(4, 4)
        }
    }

    impl DrawTarget for TestFb {
        type Color = Rgb565;
        type Error = core::convert::Infallible;
        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(p, c) in pixels {
                if let Some(i) = Self::index(p) {
                    self.pixels[i] = c;
                }
            }
            Ok(())
        }
    }

    impl ReadbackTarget for TestFb {
        fn read_pixel(&self, point: Point) -> Option<Self::Color> {
            Self::index(point).map(|i| self.pixels[i])
        }
    }

    #[test]
    fn read_pixel_round_trips_a_write() {
        let mut fb = TestFb::new();
        Pixel(Point::new(1, 2), Rgb565::RED).draw(&mut fb).unwrap();
        assert_eq!(fb.read_pixel(Point::new(1, 2)), Some(Rgb565::RED));
        assert_eq!(fb.read_pixel(Point::new(0, 0)), Some(Rgb565::BLACK));
    }

    #[test]
    fn read_pixel_out_of_bounds_is_none() {
        let fb = TestFb::new();
        assert_eq!(fb.read_pixel(Point::new(-1, 0)), None);
        assert_eq!(fb.read_pixel(Point::new(4, 0)), None);
        assert_eq!(fb.read_pixel(Point::new(0, 4)), None);
    }

    #[test]
    fn read_area_default_fills_row_major() {
        let mut fb = TestFb::new();
        Pixel(Point::new(0, 0), Rgb565::RED).draw(&mut fb).unwrap();
        Pixel(Point::new(1, 0), Rgb565::GREEN)
            .draw(&mut fb)
            .unwrap();
        Pixel(Point::new(0, 1), Rgb565::BLUE).draw(&mut fb).unwrap();

        let mut out = [Rgb565::WHITE; 4];
        let n = fb.read_area(&Rectangle::new(Point::zero(), Size::new(2, 2)), &mut out);
        assert_eq!(n, 4);
        assert_eq!(
            out,
            [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::BLACK]
        );
    }

    #[test]
    fn read_area_counts_only_in_bounds() {
        let fb = TestFb::new();
        // A region straddling the right edge: only the in-bounds column counts.
        let mut out = [Rgb565::WHITE; 4];
        let n = fb.read_area(&Rectangle::new(Point::new(3, 0), Size::new(2, 2)), &mut out);
        assert_eq!(n, 2);
    }
}
