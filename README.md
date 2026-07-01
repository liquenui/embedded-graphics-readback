# embedded-graphics-readback

A pixel-readback extension trait for [embedded-graphics].

`embedded-graphics`' `DrawTarget` is write-only — it pushes pixels but never
reads them. Targets backed by a framebuffer in RAM (a buffered display driver,
a software canvas, the simulator) *can* report what's at a pixel.
`ReadbackTarget` is the opt-in capability that exposes that, so
destination-aware rendering — antialiased compositing, blend modes,
read-modify-write effects — can sample the real backdrop instead of guessing.

```rust
use embedded_graphics_core::geometry::Point;
use embedded_graphics_readback::ReadbackTarget;

fn sample_backdrop<T: ReadbackTarget>(target: &T, p: Point) -> Option<T::Color> {
    target.read_pixel(p)
}
```

## Implementing it

Implement `read_pixel` for any target that retains its pixels; `read_area`
(a row-major bulk read) has a default that loops `read_pixel`, overridable for
framebuffers that can copy a region out in one go.

```rust,ignore
impl ReadbackTarget for MyFramebuffer {
    fn read_pixel(&self, point: Point) -> Option<Self::Color> {
        // ...look up the pixel, or None if out of bounds
    }
}
```

`read_pixel` is signature-compatible with `embedded-graphics-core`'s standard
`GetPixel::pixel`, so a target that already implements `GetPixel` can delegate
to it in one line.

Streaming targets that forward straight to a bus cannot read back and should
not implement the trait.

## no_std

The crate is `#![no_std]` and depends only on `embedded-graphics-core`.

## License

Licensed under either of MIT or Apache-2.0 at your option.

[embedded-graphics]: https://docs.rs/embedded-graphics
