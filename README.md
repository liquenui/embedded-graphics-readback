# embedded-graphics-readback

A pixel-readback capability for [embedded-graphics] draw targets.

Compositing needs to know what is already on the target before it writes.
Antialiased edges, translucent fills, blend modes and every other
read-modify-write effect blend a foreground colour _into_ a backdrop, and the
backdrop is whatever the target currently holds. `ReadbackTarget` is the
capability that says a target can answer that question, so rendering code can
require it generically:

```rust
use embedded_graphics_core::{draw_target::DrawTarget, primitives::Rectangle};
use embedded_graphics_readback::ReadbackTarget;

/// Blend `fg` over one scanline run at the given per-pixel coverage.
fn composite_run<D: ReadbackTarget>(
    target: &mut D,
    run: &Rectangle,
    fg: D::Color,
    coverage: &[u8],
    scratch: &mut [D::Color],
) {
    target.read_area(run, scratch);
    for (px, &cov) in scratch.iter_mut().zip(coverage) {
        *px = blend_over(fg, *px, cov);
    }
    let _ = target.fill_contiguous(run, scratch.iter().copied());
}
```

`ReadbackTarget: DrawTarget`, so a single bound supplies both halves of that
loop and a single `Self::Color` names the pixel type throughout. The immutable
read finishes before the mutable write begins, so one `&mut D` carries the whole
operation — no staging buffer for the shape's bounding box, just a scratch run.

## embedded-graphics framebuffers

[`Framebuffer`] is supported out of the box under the `framebuffer` feature:

```toml
embedded-graphics-readback = { version = "0.1", features = ["framebuffer"] }
```

```rust,ignore
let mut fb = Framebuffer::<Rgb565, _, LittleEndian, 64, 64, {buffer_size::<Rgb565>(64, 64)}>::new();

fb.bounding_box()
    .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
    .draw(&mut fb)?;

assert_eq!(fb.read_pixel(Point::new(10, 10)), Some(Rgb565::RED));
```

The impl covers every colour type and data order embedded-graphics makes
drawable, and overrides `read_area` to build the backing image once per region.
Draw into a `Framebuffer`, hand it to any `ReadbackTarget` renderer, and flush
it to the panel — which also makes it the shortest route to readback for a
streaming driver.

## Implementing it

For your own target, implement `read_pixel` and the rest follows:

```rust,ignore
impl ReadbackTarget for MyFramebuffer {
    fn read_pixel(&self, point: Point) -> Option<Self::Color> {
        self.index(point).map(|i| self.pixels[i])
    }
}
```

It shares its signature with [`GetPixel::pixel`], so a target that already
implements the standard trait delegates in one line:

```rust,ignore
fn read_pixel(&self, point: Point) -> Option<Self::Color> {
    GetPixel::pixel(self, point)
}
```

A reader that returns a bare colour needs a bounds guard, since `read_pixel`
answers `None` outside the bounding box:

```rust,ignore
fn read_pixel(&self, p: Point) -> Option<Rgb888> {
    self.bounding_box().contains(p).then(|| self.0.get_pixel(p))
}
```

### Bulk reads

[`read_area`] copies a region out row-major in one call. The default loops
`read_pixel`; override it whenever the target can hand over a slice, which is
the case for most framebuffers:

```rust,ignore
fn read_area(&self, area: &Rectangle, out: &mut [Self::Color]) -> usize {
    // one memcpy per row instead of one call per pixel
}
```

Because the capability belongs to the _target_, a wrapper carries it forward by
delegating both methods — that is how a clipping, translating or offscreen layer
keeps a pipeline readback-capable end to end. embedded-graphics' own
[`DrawTargetExt`] adapters hold their parent privately, so a pipeline that reads
through a layer supplies its own.

## Who implements it

- **Framebuffers and canvases** — anything holding pixels in RAM, where a read
  is a slice index. The natural implementors.
- **Buffered display drivers** that expose their buffer.
- **Simulators and test harnesses**, via a newtype.

Streaming drivers hold no pixels — they forward straight to the bus — so they
leave the trait unimplemented and callers keep the write-only path.

## Cost contract

Renderers call `read_area` once per run on the hot path and assume a read costs
about what a write costs. That holds for RAM-backed framebuffers, where a read is
slice arithmetic. Implement this trait where reads are cheap, and override
`read_area` with a block copy whenever the target allows it.

## no_std

`#![no_std]`, with `embedded-graphics-core` as its only required dependency. The
`framebuffer` feature adds `embedded-graphics` itself.

## License

Licensed under either of MIT or Apache-2.0 at your option.

[embedded-graphics]: https://docs.rs/embedded-graphics
[`Framebuffer`]: https://docs.rs/embedded-graphics/latest/embedded_graphics/framebuffer/struct.Framebuffer.html
[`DrawTargetExt`]: https://docs.rs/embedded-graphics/latest/embedded_graphics/draw_target/trait.DrawTargetExt.html
[`read_area`]: https://docs.rs/embedded-graphics-readback/latest/embedded_graphics_readback/trait.ReadbackTarget.html#method.read_area
[`GetPixel::pixel`]: https://docs.rs/embedded-graphics-core/latest/embedded_graphics_core/image/trait.GetPixel.html#tymethod.pixel
