//! Raster-to-vector tracing for the board: the tracing core of the Univers42 fork of
//! vtracer, and what the board needs on top of it.
//!
//! One trace, two outputs, because the dialog offers both:
//!
//! - **a picture** — the trace as one SVG ([`Tracer::render`] → [`Rendered::svg`]), placed
//!   on the board as a single image element;
//! - **editable shapes** — every traced region as a closed polygon
//!   ([`Rendered::shapes`]), which draw-engine turns into ordinary filled lines.
//!
//! The second needs work the tracer does not do. vtracer's regions are cubic curves with
//! holes, filled by SVG's non-zero rule; a line element is one closed polyline. So the
//! curves are flattened ([`rings::flatten_cubic`]) and each hole is spliced into the ring
//! around it as a zero-width keyhole ([`rings::compose`]). Stacking spares that work for
//! colour, whose layers the fork paints solid, but not for line art: only the dark parts
//! are traced there, so the paper inside an `O` is a hole (`tests/trace.rs`).
//!
//! Nothing here touches the network, the DOM or a clock, so all of it is tested as plain
//! Rust. The browser binding is `wasm.rs`, built only for `wasm32`.

pub mod config;
pub mod rings;
mod tracer;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use config::{PresetName, TraceConfig, TraceMode};
pub use rings::{FlatRings, ShapesPayload, TraceShape};
pub use tracer::{Rendered, TraceStats, Tracer};

/// How far a flattened curve may stray from the true one, in traced pixels.
///
/// A quarter of a pixel is below what antialiasing can show at the size the image was
/// traced at. Zoomed far past that size the facets can show; the picture keeps the curves.
pub const FLATTEN_TOLERANCE: f64 = 0.25;

/// The most points one element may carry — `MAX_POINTS_PER_ELEMENT` in
/// `packages/contract/src/limits.ts`. A ring that would exceed it is simplified before its
/// holes are spliced in, which is the only order that keeps the keyholes zero-width.
pub const MAX_RING_POINTS: usize = 10_000;
