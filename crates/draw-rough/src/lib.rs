//! A faithful Rust port of **rough.js 4.6.4** — the exact version Excalidraw pins.
//!
//! Excalidraw's entire visual identity is rough.js: every shape is generated from the
//! element's `seed` through rough's seeded PRNG. Reproducing that generator exactly is
//! what lets drawnosaurus look identical rather than merely similar, and — because the
//! output is a list of numbers — lets "identical" be *asserted* in CI rather than eyeballed.
//!
//! # What "identical" means here
//!
//! Parity is defined at the **op level**, not the pixel level: same op kinds, same
//! count, same order, coordinates equal within `1e-9`. Pixels depend on the rasterizer,
//! the device pixel ratio and font rendering; ops depend only on this code. The
//! structural part is exact; the tolerance exists solely because `sin`/`cos`/`tan` are
//! not exactly specified by IEEE-754 and V8's may differ from Rust's in the last ulp.
//! Everything else — `+ - * /` and `sqrt` — is exactly specified and matches bit for bit.
//!
//! # Maintaining this crate
//!
//! Do not "clean up" the arithmetic. See the rules at the top of [`renderer`]; each one
//! has a way of looking like an improvement while silently changing the output.
//!
//! Upstream is MIT, © 2019 Preet Shihn. See `NOTICE`.

// FMA is more accurate than a separate multiply and add, and therefore *different*.
// Clippy suggests it by default; taking that suggestion would break parity in a way
// that no structural test catches — only the coordinate diff would notice, and only
// sometimes. This lint stays denied crate-wide.
#![allow(clippy::suboptimal_flops)]
// Transcribed formulae read closest to the original when left as written.
#![allow(clippy::excessive_precision)]

pub mod fillers;
pub mod generator;
pub mod geometry;
pub mod hachure;
pub mod jsnum;
pub mod ops;
pub mod options;
pub mod points_on_curve;
pub mod renderer;
pub mod rng;

pub use ops::{Drawable, Op, OpSet, OpSetKind};
pub use options::{Ctx, FillStyle, Options};
pub use rng::Random;
