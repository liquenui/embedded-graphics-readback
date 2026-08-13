//! Adapters that carry [`ReadbackTarget`] down a pipeline.
//!
//! `embedded-graphics`' own [`DrawTargetExt`] adapters keep their parent in a
//! private field and expose no accessor, so readback cannot be delegated
//! through them. These are read-capable equivalents: each implements
//! [`DrawTarget`] with the same write semantics as its `embedded-graphics`
//! counterpart, and [`ReadbackTarget`] on top.
//!
//! [`ReadbackTargetExt`] is blanket-implemented, so the adapters are themselves
//! readback targets and chain like the originals:
//!
//! ```
//! use embedded_graphics_core::{geometry::Point, primitives::Rectangle};
//! use embedded_graphics_readback::{ReadbackTarget, ReadbackTargetExt};
//!
//! # use embedded_graphics_core::{
//! #     Pixel, draw_target::DrawTarget, geometry::{OriginDimensions, Size},
//! #     pixelcolor::{Rgb565, RgbColor},
//! # };
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
//! #     fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
//! #     where I: IntoIterator<Item = Pixel<Self::Color>> {
//! #         for Pixel(p, c) in pixels {
//! #             if let Some(i) = Fb::index(p) { self.0[i] = c; }
//! #         }
//! #         Ok(())
//! #     }
//! # }
//! # impl ReadbackTarget for Fb {
//! #     fn read_pixel(&self, point: Point) -> Option<Rgb565> {
//! #         Fb::index(point).map(|i| self.0[i])
//! #     }
//! # }
//! let mut fb = Fb([Rgb565::BLACK; 64]);
//! assert_eq!(fb.read_pixel(Point::new(4, 4)), Some(Rgb565::BLACK));
//!
//! // Shift the origin, then mask: the capability survives both layers.
//! let mut layer = fb.shifted(Point::new(2, 2));
//! let view = layer.masked(&Rectangle::new(Point::zero(), Size::new(3, 3)));
//!
//! // (0, 0) in the view is (2, 2) in the framebuffer.
//! assert_eq!(view.read_pixel(Point::zero()), Some(Rgb565::BLACK));
//! // Outside the mask, even though the framebuffer still has a pixel there.
//! assert_eq!(view.read_pixel(Point::new(4, 4)), None);
//! ```
//!
//! [`DrawTargetExt`]:
//!     https://docs.rs/embedded-graphics/latest/embedded_graphics/draw_target/trait.DrawTargetExt.html

use embedded_graphics_core::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{Dimensions, OriginDimensions, Point, Size},
    primitives::{PointsIter, Rectangle},
};

use crate::{ReadbackTarget, read_area_by_pixel};

/// Shift a rectangle; `Transform` lives in `embedded-graphics`, not core.
#[inline]
fn translate(area: &Rectangle, offset: Point) -> Rectangle {
    Rectangle::new(area.top_left + offset, area.size)
}

/// Adapters for [`ReadbackTarget`], mirroring `embedded-graphics`'
/// `DrawTargetExt`.
///
/// Blanket-implemented for every [`ReadbackTarget`], including the adapters
/// themselves, so layers compose. The names differ from `DrawTargetExt`'s so
/// both traits can sit in scope without ambiguous calls; each method is
/// shorthand for the matching struct constructor.
pub trait ReadbackTargetExt: ReadbackTarget + Sized {
    /// Translate the target's origin by `offset`.
    ///
    /// Point `p` on the adapter addresses `p + offset` on the parent.
    #[must_use]
    fn shifted(&mut self, offset: Point) -> Shifted<'_, Self>;

    /// Restrict the target to `area`, moving the origin to `area.top_left`.
    ///
    /// `area` is clamped to the parent's bounding box first.
    #[must_use]
    fn windowed(&mut self, area: &Rectangle) -> Windowed<'_, Self>;

    /// Restrict the target to `area`, keeping the parent's coordinates.
    ///
    /// `area` is clamped to the parent's bounding box first.
    #[must_use]
    fn masked(&mut self, area: &Rectangle) -> Masked<'_, Self>;
}

impl<T> ReadbackTargetExt for T
where
    T: ReadbackTarget,
{
    fn shifted(&mut self, offset: Point) -> Shifted<'_, Self> {
        Shifted::new(self, offset)
    }

    fn windowed(&mut self, area: &Rectangle) -> Windowed<'_, Self> {
        Windowed::new(self, area)
    }

    fn masked(&mut self, area: &Rectangle) -> Masked<'_, Self> {
        Masked::new(self, area)
    }
}

/// A target with its origin moved, from [`ReadbackTargetExt::shifted`].
#[derive(Debug)]
pub struct Shifted<'a, T> {
    parent: &'a mut T,
    offset: Point,
}

impl<'a, T> Shifted<'a, T> {
    /// Wrap `parent`, shifting every coordinate that crosses this layer by
    /// `offset`.
    #[must_use]
    pub fn new(parent: &'a mut T, offset: Point) -> Self {
        Self { parent, offset }
    }

    /// The target underneath this layer.
    #[must_use]
    pub fn parent(&self) -> &T {
        self.parent
    }

    /// The offset applied to every coordinate crossing this layer.
    #[must_use]
    pub fn offset(&self) -> Point {
        self.offset
    }
}

impl<T> DrawTarget for Shifted<'_, T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let offset = self.offset;

        self.parent
            .draw_iter(pixels.into_iter().map(|Pixel(p, c)| Pixel(p + offset, c)))
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let area = translate(area, self.offset);

        self.parent.fill_contiguous(&area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let area = translate(area, self.offset);

        self.parent.fill_solid(&area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.clear(color)
    }
}

impl<T> Dimensions for Shifted<'_, T>
where
    T: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        translate(&self.parent.bounding_box(), -self.offset)
    }
}

impl<T> ReadbackTarget for Shifted<'_, T>
where
    T: ReadbackTarget,
{
    fn read_pixel(&self, point: Point) -> Option<Self::Color> {
        self.parent.read_pixel(point + self.offset)
    }

    fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
        // A pure coordinate shift, so the parent's bulk path survives intact.
        self.parent.read_area(&translate(area, self.offset), out)
    }
}

/// A target restricted to a sub-region, from [`ReadbackTargetExt::windowed`].
#[derive(Debug)]
pub struct Windowed<'a, T> {
    parent: Shifted<'a, T>,
    size: Size,
}

impl<'a, T> Windowed<'a, T>
where
    T: DrawTarget,
{
    /// Wrap `parent`, restricting it to `area` and moving the origin to
    /// `area.top_left`. `area` is clamped to the parent's bounding box.
    #[must_use]
    pub fn new(parent: &'a mut T, area: &Rectangle) -> Self {
        let area = area.intersection(&parent.bounding_box());

        Self {
            size: area.size,
            parent: Shifted::new(parent, area.top_left),
        }
    }
}

impl<T> Windowed<'_, T> {
    /// The target underneath this layer.
    #[must_use]
    pub fn parent(&self) -> &T {
        self.parent.parent()
    }

    /// The window's top-left corner, in the parent's coordinates.
    #[must_use]
    pub fn offset(&self) -> Point {
        self.parent.offset()
    }
}

impl<T> DrawTarget for Windowed<'_, T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.parent.draw_iter(pixels)
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.parent.fill_contiguous(area, colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.parent.fill_solid(area, color)
    }
}

impl<T> OriginDimensions for Windowed<'_, T>
where
    T: DrawTarget,
{
    fn size(&self) -> Size {
        self.size
    }
}

impl<T> ReadbackTarget for Windowed<'_, T>
where
    T: ReadbackTarget,
{
    fn read_pixel(&self, point: Point) -> Option<Self::Color> {
        // Reads honour the window, unlike writes: `read_pixel` promises `None`
        // outside the bounding box.
        if self.bounding_box().contains(point) {
            self.parent.read_pixel(point)
        } else {
            None
        }
    }

    fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
        if self.bounding_box().intersection(area) == *area {
            self.parent.read_area(area, out)
        } else {
            read_area_by_pixel(self, area, out)
        }
    }
}

/// A target masked to a region, from [`ReadbackTargetExt::masked`].
#[derive(Debug)]
pub struct Masked<'a, T> {
    parent: &'a mut T,
    mask_area: Rectangle,
}

impl<'a, T> Masked<'a, T>
where
    T: DrawTarget,
{
    /// Wrap `parent`, admitting only the pixels inside `area`.
    ///
    /// `area` is clamped to the parent's bounding box.
    #[must_use]
    pub fn new(parent: &'a mut T, area: &Rectangle) -> Self {
        let mask_area = area.intersection(&parent.bounding_box());

        Self { parent, mask_area }
    }
}

impl<T> Masked<'_, T> {
    /// The target underneath this layer.
    #[must_use]
    pub fn parent(&self) -> &T {
        self.parent
    }

    /// The region this layer admits, in the parent's coordinates.
    #[must_use]
    pub fn mask_area(&self) -> Rectangle {
        self.mask_area
    }
}

impl<T> DrawTarget for Masked<'_, T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let mask_area = self.mask_area;

        self.parent.draw_iter(
            pixels
                .into_iter()
                .filter(|Pixel(p, _)| mask_area.contains(*p)),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        if self.mask_area.intersection(area) == *area {
            self.parent.fill_contiguous(area, colors)
        } else {
            // Partial overlap: pair each colour with its point and let
            // `draw_iter` drop the ones outside the mask.
            self.draw_iter(area.points().zip(colors).map(|(p, c)| Pixel(p, c)))
        }
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let area = area.intersection(&self.mask_area);

        self.parent.fill_solid(&area, color)
    }
}

impl<T> Dimensions for Masked<'_, T>
where
    T: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        self.mask_area
    }
}

impl<T> ReadbackTarget for Masked<'_, T>
where
    T: ReadbackTarget,
{
    fn read_pixel(&self, point: Point) -> Option<Self::Color> {
        if self.mask_area.contains(point) {
            self.parent.read_pixel(point)
        } else {
            None
        }
    }

    fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
        if self.mask_area.intersection(area) == *area {
            self.parent.read_area(area, out)
        } else {
            read_area_by_pixel(self, area, out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::prelude::*;
    use embedded_graphics_core::pixelcolor::Rgb565;

    /// An 8×8 framebuffer that records how many bulk reads it served, so the
    /// adapters' fast paths can be observed rather than assumed.
    struct Fb {
        pixels: [Rgb565; 64],
        bulk_reads: core::cell::Cell<usize>,
    }

    impl Fb {
        fn new() -> Self {
            Self {
                pixels: [Rgb565::BLACK; 64],
                bulk_reads: core::cell::Cell::new(0),
            }
        }

        fn index(p: Point) -> Option<usize> {
            ((0..8).contains(&p.x) && (0..8).contains(&p.y)).then(|| (p.y * 8 + p.x) as usize)
        }
    }

    impl OriginDimensions for Fb {
        fn size(&self) -> Size {
            Size::new(8, 8)
        }
    }

    impl DrawTarget for Fb {
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

    impl ReadbackTarget for Fb {
        fn read_pixel(&self, point: Point) -> Option<Self::Color> {
            Self::index(point).map(|i| self.pixels[i])
        }

        fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
            self.bulk_reads.set(self.bulk_reads.get() + 1);
            read_area_by_pixel(self, area, out)
        }
    }

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    // --- shifted ------------------------------------------------------

    #[test]
    fn shifted_reads_through_the_offset() {
        let mut fb = Fb::new();
        Pixel(Point::new(5, 6), Rgb565::RED).draw(&mut fb).unwrap();

        let layer = fb.shifted(Point::new(3, 4));

        assert_eq!(layer.read_pixel(Point::new(2, 2)), Some(Rgb565::RED));
        assert_eq!(layer.read_pixel(Point::new(0, 0)), Some(Rgb565::BLACK));
    }

    #[test]
    fn shifted_round_trips_a_write_through_the_adapter() {
        let mut fb = Fb::new();
        let mut layer = fb.shifted(Point::new(3, 4));

        Pixel(Point::new(1, 1), Rgb565::GREEN)
            .draw(&mut layer)
            .unwrap();

        assert_eq!(layer.read_pixel(Point::new(1, 1)), Some(Rgb565::GREEN));
        // ...and it landed at the shifted position underneath.
        assert_eq!(fb.read_pixel(Point::new(4, 5)), Some(Rgb565::GREEN));
    }

    #[test]
    fn shifted_moves_the_bounding_box() {
        let mut fb = Fb::new();
        let layer = fb.shifted(Point::new(3, 4));

        assert_eq!(layer.bounding_box(), rect(-3, -4, 8, 8));
    }

    #[test]
    fn shifted_read_area_keeps_the_parent_bulk_path() {
        let mut fb = Fb::new();
        Pixel(Point::new(3, 4), Rgb565::RED).draw(&mut fb).unwrap();

        let layer = fb.shifted(Point::new(3, 4));
        let mut out = [Rgb565::WHITE; 4];
        let n = layer.read_area(&rect(0, 0, 2, 2), &mut out);

        assert_eq!(n, 4);
        assert_eq!(out[0], Rgb565::RED);
        // Exactly one bulk call reached the framebuffer: no per-pixel fallback.
        assert_eq!(layer.parent().bulk_reads.get(), 1);
    }

    // --- windowed ---------------------------------------------------------

    #[test]
    fn windowed_moves_the_origin_and_reports_its_size() {
        let mut fb = Fb::new();
        Pixel(Point::new(2, 3), Rgb565::RED).draw(&mut fb).unwrap();

        let view = fb.windowed(&rect(2, 3, 4, 4));

        assert_eq!(view.bounding_box(), rect(0, 0, 4, 4));
        assert_eq!(view.offset(), Point::new(2, 3));
        assert_eq!(view.read_pixel(Point::zero()), Some(Rgb565::RED));
    }

    #[test]
    fn windowed_reads_are_none_outside_the_window() {
        let mut fb = Fb::new();
        let view = fb.windowed(&rect(2, 2, 3, 3));

        // (3, 3) in view space is (5, 5) in the framebuffer, which still holds a
        // pixel — but it is outside this target's bounding box.
        assert_eq!(view.read_pixel(Point::new(3, 3)), None);
        assert_eq!(view.read_pixel(Point::new(-1, 0)), None);
    }

    #[test]
    fn windowed_clamps_to_the_parent() {
        let mut fb = Fb::new();
        let view = fb.windowed(&rect(6, 6, 8, 8));

        assert_eq!(view.bounding_box(), rect(0, 0, 2, 2));
    }

    #[test]
    fn windowed_read_area_straddling_the_edge_counts_only_the_window() {
        let mut fb = Fb::new();
        let view = fb.windowed(&rect(0, 0, 2, 2));

        let mut out = [Rgb565::WHITE; 4];
        let n = view.read_area(&rect(1, 1, 2, 2), &mut out);

        // Only (1, 1) is inside the 2×2 window.
        assert_eq!(n, 1);
        // The slow path ran, so the parent saw no bulk read.
        assert_eq!(view.parent().bulk_reads.get(), 0);
    }

    // --- masked ---------------------------------------------------------

    #[test]
    fn masked_keeps_parent_coordinates() {
        let mut fb = Fb::new();
        Pixel(Point::new(4, 4), Rgb565::RED).draw(&mut fb).unwrap();

        let view = fb.masked(&rect(2, 2, 4, 4));

        assert_eq!(view.read_pixel(Point::new(4, 4)), Some(Rgb565::RED));
        assert_eq!(view.bounding_box(), rect(2, 2, 4, 4));
    }

    #[test]
    fn masked_reads_are_none_outside_the_mask() {
        let mut fb = Fb::new();
        let view = fb.masked(&rect(2, 2, 4, 4));

        assert_eq!(view.read_pixel(Point::new(1, 1)), None);
        assert_eq!(view.read_pixel(Point::new(6, 6)), None);
    }

    #[test]
    fn masked_clamps_to_the_parent() {
        let mut fb = Fb::new();
        let view = fb.masked(&rect(4, 4, 100, 100));

        // The mask cannot report more pixels than the parent holds.
        assert_eq!(view.bounding_box(), rect(4, 4, 4, 4));
        assert_eq!(view.mask_area(), rect(4, 4, 4, 4));
    }

    #[test]
    fn masked_drops_writes_outside_the_mask() {
        let mut fb = Fb::new();
        let mut view = fb.masked(&rect(2, 2, 2, 2));

        view.fill_solid(&rect(0, 0, 8, 8), Rgb565::RED).unwrap();

        assert_eq!(fb.read_pixel(Point::new(2, 2)), Some(Rgb565::RED));
        assert_eq!(fb.read_pixel(Point::new(0, 0)), Some(Rgb565::BLACK));
    }

    #[test]
    fn masked_fill_contiguous_crops_a_straddling_run() {
        let mut fb = Fb::new();
        let mut view = fb.masked(&rect(0, 0, 2, 8));

        // A 4-wide run: the right half falls outside the mask.
        view.fill_contiguous(
            &rect(0, 0, 4, 1),
            [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE],
        )
        .unwrap();

        assert_eq!(fb.read_pixel(Point::new(0, 0)), Some(Rgb565::RED));
        assert_eq!(fb.read_pixel(Point::new(1, 0)), Some(Rgb565::GREEN));
        // Colours stay paired with their points; the masked ones are dropped.
        assert_eq!(fb.read_pixel(Point::new(2, 0)), Some(Rgb565::BLACK));
        assert_eq!(fb.read_pixel(Point::new(3, 0)), Some(Rgb565::BLACK));
    }

    #[test]
    fn masked_read_area_wholly_inside_uses_the_parent_bulk_path() {
        let mut fb = Fb::new();
        let view = fb.masked(&rect(0, 0, 8, 8));

        let mut out = [Rgb565::WHITE; 4];
        assert_eq!(view.read_area(&rect(1, 1, 2, 2), &mut out), 4);
        assert_eq!(view.parent().bulk_reads.get(), 1);
    }

    // --- composition -----------------------------------------------------

    #[test]
    fn adapters_chain_and_keep_readback() {
        let mut fb = Fb::new();
        Pixel(Point::new(2, 2), Rgb565::RED).draw(&mut fb).unwrap();

        let mut layer = fb.shifted(Point::new(2, 2));
        let view = layer.masked(&rect(0, 0, 3, 3));

        // (0, 0) through both layers is (2, 2) on the framebuffer.
        assert_eq!(view.read_pixel(Point::zero()), Some(Rgb565::RED));
        assert_eq!(view.read_pixel(Point::new(4, 4)), None);
    }

    #[test]
    fn chained_read_area_agrees_with_read_pixel() {
        let mut fb = Fb::new();
        Pixel(Point::new(3, 3), Rgb565::GREEN)
            .draw(&mut fb)
            .unwrap();

        let mut layer = fb.shifted(Point::new(2, 2));
        let view = layer.windowed(&rect(0, 0, 4, 4));

        let mut out = [Rgb565::WHITE; 4];
        let n = view.read_area(&rect(0, 0, 2, 2), &mut out);

        assert_eq!(n, 4);
        let by_pixel: [Rgb565; 4] = core::array::from_fn(|i| {
            view.read_pixel(Point::new(i as i32 % 2, i as i32 / 2))
                .unwrap()
        });
        assert_eq!(out, by_pixel);
        assert_eq!(out[3], Rgb565::GREEN);
    }
}
