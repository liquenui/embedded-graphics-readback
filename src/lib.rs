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
//! ```
//! use embedded_graphics_core::{draw_target::DrawTarget, primitives::Rectangle};
//! use embedded_graphics_readback::ReadbackTarget;
//!
//! # fn blend_over<C>(fg: C, _bg: C, _coverage: u8) -> C { fg }
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
//! [`read_pixel`](ReadbackTarget::read_pixel) is the one required method, and
//! [`read_area`](ReadbackTarget::read_area) copies a region out row-major in
//! one call — defaulting to a `read_pixel` loop, overridable with a block copy.
//!
//! # Layers
//!
//! Because the capability belongs to the *target*, a wrapper carries it forward
//! by delegating both methods. [`adapters`] ships the three that matter —
//! [`Shifted`](adapters::Shifted), [`Windowed`](adapters::Windowed) and
//! [`Masked`](adapters::Masked) — so a pipeline keeps readback end to end:
//!
//! ```
//! # use embedded_graphics_core::{
//! #     Pixel, draw_target::DrawTarget, geometry::{OriginDimensions, Point, Size},
//! #     pixelcolor::{Rgb565, RgbColor}, primitives::Rectangle,
//! # };
//! # use embedded_graphics_readback::ReadbackTarget;
//! use embedded_graphics_readback::ReadbackTargetExt;
//!
//! # struct Fb([Rgb565; 64]);
//! # impl Fb {
//! #     fn index(p: Point) -> Option<usize> {
//! #         ((0..8).contains(&p.x) && (0..8).contains(&p.y)).then(|| (p.y * 8 + p.x) as usize)
//! #     }
//! # }
//! # impl OriginDimensions for Fb {
//! #     fn size(&self) -> Size { Size::new(8, 8) }
//! # }
//! # impl DrawTarget for Fb {
//! #     type Color = Rgb565;
//! #     type Error = core::convert::Infallible;
//! #     fn draw_iter<I>(&mut self, _: I) -> Result<(), Self::Error>
//! #     where I: IntoIterator<Item = Pixel<Self::Color>> { Ok(()) }
//! # }
//! # impl ReadbackTarget for Fb {
//! #     fn read_pixel(&self, p: Point) -> Option<Rgb565> {
//! #         Fb::index(p).map(|i| self.0[i])
//! #     }
//! # }
//! # let mut fb = Fb([Rgb565::BLACK; 64]);
//! let mut layer = fb.shifted(Point::new(2, 2));
//! let mut view = layer.masked(&Rectangle::new(Point::zero(), Size::new(4, 4)));
//!
//! composite_run(&mut view, /* ... */);
//! # fn composite_run<D: ReadbackTarget>(_: &mut D) {}
//! ```
//!
//! `embedded-graphics`' own [`DrawTargetExt`] adapters hold their parent in a
//! private field with no accessor, so readback cannot be delegated through
//! them — these are read-capable equivalents with the same write semantics.
//!
//! # Implementing it
//!
//! [`read_pixel`](ReadbackTarget::read_pixel) shares its signature with
//! [`GetPixel::pixel`](embedded_graphics_core::image::GetPixel::pixel), so a
//! target that already implements that trait delegates in one line. A reader
//! that hands back a bare colour needs a bounds guard instead, since
//! `read_pixel` answers `None` outside the bounding box:
//!
//! ```
//! use embedded_graphics_core::{
//!     draw_target::DrawTarget,
//!     geometry::{Dimensions, Point},
//!     image::GetPixel,
//!     pixelcolor::Rgb565,
//! };
//! use embedded_graphics_readback::ReadbackTarget;
//!
//! # use embedded_graphics_core::{geometry::Size, primitives::Rectangle, Pixel};
//! # struct Canvas;
//! # impl Canvas {
//! #     fn get_pixel(&self, _p: Point) -> Rgb565 { Rgb565::new(0, 0, 0) }
//! # }
//! # struct Fb;
//! # struct Sim(Canvas);
//! # impl Dimensions for Fb {
//! #     fn bounding_box(&self) -> Rectangle { Rectangle::new(Point::zero(), Size::new(64, 64)) }
//! # }
//! # impl DrawTarget for Fb {
//! #     type Color = Rgb565;
//! #     type Error = core::convert::Infallible;
//! #     fn draw_iter<I>(&mut self, _: I) -> Result<(), Self::Error>
//! #     where I: IntoIterator<Item = Pixel<Self::Color>> { Ok(()) }
//! # }
//! # impl GetPixel for Fb {
//! #     type Color = Rgb565;
//! #     fn pixel(&self, _p: Point) -> Option<Rgb565> { None }
//! # }
//! # impl Dimensions for Sim {
//! #     fn bounding_box(&self) -> Rectangle { Rectangle::new(Point::zero(), Size::new(64, 64)) }
//! # }
//! # impl DrawTarget for Sim {
//! #     type Color = Rgb565;
//! #     type Error = core::convert::Infallible;
//! #     fn draw_iter<I>(&mut self, _: I) -> Result<(), Self::Error>
//! #     where I: IntoIterator<Item = Pixel<Self::Color>> { Ok(()) }
//! # }
//! // Already a `GetPixel`: delegate.
//! impl ReadbackTarget for Fb {
//!     fn read_pixel(&self, point: Point) -> Option<Self::Color> {
//!         GetPixel::pixel(self, point)
//!     }
//! }
//!
//! // Hands back a bare colour: guard on the bounding box.
//! impl ReadbackTarget for Sim {
//!     fn read_pixel(&self, point: Point) -> Option<Self::Color> {
//!         self.bounding_box()
//!             .contains(point)
//!             .then(|| self.0.get_pixel(point))
//!     }
//! }
//! ```
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
//! `ReadbackTarget` renderer, and flush it to the panel — which also makes it
//! the shortest route to readback for a streaming driver.
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
//! [`DrawTargetExt`]:
//!     https://docs.rs/embedded-graphics/latest/embedded_graphics/draw_target/trait.DrawTargetExt.html
//!
//! [`embedded-graphics`]: https://docs.rs/embedded-graphics
//! [`embedded-graphics-core`]: https://docs.rs/embedded-graphics-core
//! [`DrawTarget`]: embedded_graphics_core::draw_target::DrawTarget

pub mod adapters;

#[cfg(feature = "framebuffer")]
#[cfg_attr(docsrs, doc(cfg(feature = "framebuffer")))]
mod framebuffer;

pub use adapters::ReadbackTargetExt;

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
/// That holds for RAM-backed framebuffers, where a read is slice arithmetic;
/// those are the intended targets. A panel whose only read primitive is a slow
/// per-pixel bus command should not implement this trait merely because it can:
/// the default [`read_area`](Self::read_area) turns every antialiased run into
/// `width` bus round-trips. Implement it where reads are cheap, and override
/// [`read_area`](Self::read_area) with a block copy whenever the target allows
/// it.
pub trait ReadbackTarget: DrawTarget {
    /// The colour currently at `point`, or `None` if `point` lies outside the
    /// target's [bounding box](embedded_graphics_core::geometry::Dimensions::bounding_box).
    ///
    /// Matches [`GetPixel::pixel`](embedded_graphics_core::image::GetPixel::pixel).
    #[must_use]
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
        read_area_by_pixel(self, area, out)
    }
}

/// The per-pixel loop behind [`ReadbackTarget::read_area`]'s default.
///
/// Shared with the [`adapters`], which override `read_area` to hand whole
/// regions to their parent and fall back to this when a region only partly
/// overlaps the layer. Deliberately not public: `embedded-graphics-core`
/// exports no helpers behind `DrawTarget`'s defaults either, and a wrapper
/// that needs the walk can write it in eight lines. Easy to export later if
/// a downstream asks; impossible to unexport once released.
pub(crate) fn read_area_by_pixel<T>(target: &T, area: &Rectangle, out: &mut [T::Color]) -> usize
where
    T: ReadbackTarget + ?Sized,
{
    let mut written = 0;
    for (slot, point) in out.iter_mut().zip(area.points()) {
        if let Some(color) = target.read_pixel(point) {
            *slot = color;
            written += 1;
        }
    }
    written
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
