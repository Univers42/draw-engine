//! rough.js `Options`, and the generation context that carries the RNG.
//!
//! Defaults transcribed from roughjs 4.6.4 `bin/generator.js` `defaultOptions`.

use crate::rng::Random;

/// rough's fill patterns. `Solid` is handled in the generator rather than by a filler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillStyle {
    Hachure,
    Solid,
    ZigZag,
    CrossHatch,
    Dots,
    Dashed,
    ZigZagLine,
}

/// rough.js's drawing options.
///
/// Only the fields that affect *geometry* are modelled. rough also carries `stroke`,
/// `fill` and `strokeWidth` for painting, but `strokeWidth` is the one that feeds back
/// into geometry (via the hachure gap), so it is kept; the colours are not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub max_randomness_offset: f64,
    pub roughness: f64,
    pub bowing: f64,
    pub stroke_width: f64,
    pub curve_tightness: f64,
    pub curve_fitting: f64,
    pub curve_step_count: f64,
    pub fill_style: FillStyle,
    /// `-1` means "derive from strokeWidth" — rough's sentinel, preserved rather than
    /// resolved early so the derivation happens at the same point it does in JS.
    pub fill_weight: f64,
    pub hachure_angle: f64,
    pub hachure_gap: f64,
    pub dash_offset: f64,
    pub dash_gap: f64,
    pub zigzag_offset: f64,
    pub seed: i32,
    pub disable_multi_stroke: bool,
    pub disable_multi_stroke_fill: bool,
    pub preserve_vertices: bool,
    pub fill_shape_roughness_gain: f64,
    /// Whether the shape has a fill at all. In JS this is `o.fill` being a non-empty
    /// colour string; geometry only needs the boolean.
    pub filled: bool,
}

impl Default for Options {
    /// `RoughGenerator.defaultOptions`, roughjs 4.6.4 `bin/generator.js:9-32`.
    fn default() -> Self {
        Self {
            max_randomness_offset: 2.0,
            roughness: 1.0,
            bowing: 1.0,
            stroke_width: 1.0,
            curve_tightness: 0.0,
            curve_fitting: 0.95,
            curve_step_count: 9.0,
            fill_style: FillStyle::Hachure,
            fill_weight: -1.0,
            hachure_angle: -41.0,
            hachure_gap: -1.0,
            dash_offset: -1.0,
            dash_gap: -1.0,
            zigzag_offset: -1.0,
            seed: 0,
            disable_multi_stroke: false,
            disable_multi_stroke_fill: false,
            preserve_vertices: false,
            fill_shape_roughness_gain: 0.8,
            filled: false,
        }
    }
}

/// Options plus the lazily-created RNG.
///
/// In JS the randomizer is stashed *on the options object* (`ops.randomizer`), so one
/// generator call shares a single random stream across every helper it reaches. That
/// sharing is load-bearing: the stream position when `_line` is entered depends on
/// everything drawn before it. Modelling it as `&mut Ctx` threaded through the
/// renderer reproduces it exactly, and makes the dependency visible in the signatures
/// instead of hidden in a field.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub o: Options,
    rng: Option<Random>,
}

impl Ctx {
    pub fn new(o: Options) -> Self {
        Self { o, rng: None }
    }

    /// `random(ops)` — creates the randomizer on first use, from `ops.seed || 0`.
    pub fn random(&mut self) -> f64 {
        if self.rng.is_none() {
            self.rng = Some(Random::new(self.o.seed));
        }
        // Unwrap is sound: just populated above.
        self.rng.as_mut().expect("randomizer initialised").next()
    }

    /// `cloneOptionsAlterSeed` — a fresh randomizer seeded one higher, used for the
    /// second pass of a multi-stroke curve or ellipse.
    ///
    /// The `if (ops.seed)` guard is faithful: a zero seed stays zero rather than
    /// becoming 1, which would turn rough's non-deterministic branch into a
    /// deterministic one and diverge.
    pub fn clone_alter_seed(&self) -> Ctx {
        let mut o = self.o;
        if o.seed != 0 {
            o.seed = o.seed.wrapping_add(1);
        }
        Ctx { o, rng: None }
    }

    /// `_offset(min, max, ops, roughnessGain)`.
    ///
    /// Transcribed shape-for-shape — `(random * (max - min)) + min` is **not**
    /// algebraically simplified, because floating-point addition is not associative
    /// and any rearrangement changes the last bits.
    pub fn offset(&mut self, min: f64, max: f64, roughness_gain: f64) -> f64 {
        let r = self.random();
        self.o.roughness * roughness_gain * ((r * (max - min)) + min)
    }

    /// `_offsetOpt(x, ops, roughnessGain)` = `_offset(-x, x, ops, roughnessGain)`.
    pub fn offset_opt(&mut self, x: f64, roughness_gain: f64) -> f64 {
        self.offset(-x, x, roughness_gain)
    }
}
