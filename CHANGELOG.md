# Changelog

All notable changes to the `embedded-graphics-readback` crate are
documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While the crate is pre-`1.0`, breaking changes bump the **minor**
version (`0.y`), per SemVer's `0.x` rule.

The crate tracks `embedded-graphics-core` `0.4`; moving to a new
`embedded-graphics-core` minor is breaking for downstream code and bumps
this crate's minor version in step.

## [0.1.0] - 2026-08-13

Initial release. Compositing — antialiased edges, translucent fills,
blend modes, any read-modify-write effect — blends a foreground colour
into whatever the target already holds, and `DrawTarget` alone cannot
report that backdrop. This crate adds the capability as a trait, so
rendering code can require readback generically instead of being written
against one concrete framebuffer.

### Added

- **`ReadbackTarget`** — a capability trait for `DrawTarget`s that retain
  their pixels. Taking `DrawTarget` as a supertrait means one bound
  supplies both halves of a read-modify-write loop and one `Self::Color`
  names the pixel type throughout, and the immutable read completes
  before the mutable write begins, so a single `&mut D` carries the whole
  operation with no bounding-box staging buffer.
- **`ReadbackTarget::read_pixel`** — the colour currently at a point, or
  `None` outside the target's bounding box. It is the only required
  method, and shares its signature with
  `embedded_graphics_core::image::GetPixel::pixel` so a target that
  already implements the standard trait delegates in one line.
- **`ReadbackTarget::read_area`** — reads a region row-major into a
  caller-supplied slice and returns how many pixels were in bounds.
  Defaults to a `read_pixel` loop and is overridable with a block copy;
  out-of-bounds slots are left untouched, so callers can pre-fill a
  fallback colour. This is the method destination-aware renderers call on
  the hot path, once per run rather than once per pixel.
- **`ReadbackTargetExt` and the `adapters` module** — `Shifted`, `Windowed` and
  `Masked` layers that implement `DrawTarget` with the same write semantics as
  their `embedded-graphics` counterparts (`Translated`, `Cropped`, `Clipped`)
  and `ReadbackTarget` on top, so a pipeline keeps the capability through a
  layer. `embedded-graphics`' own `DrawTargetExt` adapters hold their parent in
  a private field with no accessor, which makes delegating readback through
  them impossible from any crate. Each layer forwards a whole region to its
  parent's `read_area` when it falls inside the layer and walks per-pixel only
  where the region straddles an edge. Reads honour the layer's bounding box
  even where writes do not, so the in-bounds count `read_area` returns stays
  meaningful. The names differ from `DrawTargetExt`'s on purpose: that trait is
  blanket-implemented for every `DrawTarget` and sits in the
  `embedded-graphics` prelude, so a shared name would leave both candidates
  applicable at every glob-importing call site. Each method is shorthand for a
  public constructor on the layer. The layers expose the `parent()` accessor
  the originals lack, so downstream code can extend them.
- **`framebuffer` feature** — implements `ReadbackTarget` for
  `embedded_graphics::framebuffer::Framebuffer`. The impl is generic over
  the intersection of `embedded-graphics`' own `DrawTarget` and
  `GetPixel` impls, so it covers every colour type and data order that
  crate makes both drawable and readable. `read_area` is overridden to
  build the backing image once per region instead of once per pixel.
  Enabling the feature is all that is required — no newtype, no manual
  impl — and rendering into a `Framebuffer` is also the shortest route to
  readback for a streaming display driver.

[0.1.0]: https://github.com/liquenui/embedded-graphics-readback/releases/tag/v0.1.0
