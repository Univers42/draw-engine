//! rough.js's output model: `Op`, `OpSet`, `Drawable`.
//!
//! rough only ever emits three op kinds — `move`, `lineTo` and `bcurveTo` — so they are
//! an enum with fixed-size payloads rather than the JS `{op, data: number[]}` shape.
//! That removes a heap allocation per op, and it is what lets a `Drawable` be encoded
//! straight into the render arena without a second pass.

/// A single path command. Operand counts are fixed by the variant, matching
/// `data.length` in the JS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// `{ op: 'move', data: [x, y] }`
    Move([f64; 2]),
    /// `{ op: 'lineTo', data: [x, y] }`
    LineTo([f64; 2]),
    /// `{ op: 'bcurveTo', data: [x1, y1, x2, y2, x, y] }`
    BCurveTo([f64; 6]),
}

impl Op {
    /// The JS `op` string, for fixture comparison and debugging.
    pub fn kind(&self) -> &'static str {
        match self {
            Op::Move(_) => "move",
            Op::LineTo(_) => "lineTo",
            Op::BCurveTo(_) => "bcurveTo",
        }
    }

    /// The operands in JS `data` order.
    pub fn data(&self) -> &[f64] {
        match self {
            Op::Move(d) | Op::LineTo(d) => d.as_slice(),
            Op::BCurveTo(d) => d.as_slice(),
        }
    }
}

/// How a set of ops is painted. Mirrors rough's `OpSetType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpSetKind {
    /// Stroked with the shape's stroke.
    Path,
    /// Filled with the shape's fill — solid fills only.
    FillPath,
    /// Stroked with the *fill* colour at `fillWeight` — this is how hachure and the
    /// other pattern fills are drawn. Named `fillSketch` in rough.
    FillSketch,
}

impl OpSetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpSetKind::Path => "path",
            OpSetKind::FillPath => "fillPath",
            OpSetKind::FillSketch => "fillSketch",
        }
    }
}

/// One `{ type, ops }` set.
#[derive(Clone, Debug, PartialEq)]
pub struct OpSet {
    pub kind: OpSetKind,
    pub ops: Vec<Op>,
}

impl OpSet {
    pub fn path(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetKind::Path,
            ops,
        }
    }
    pub fn fill_path(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetKind::FillPath,
            ops,
        }
    }
    pub fn fill_sketch(ops: Vec<Op>) -> Self {
        Self {
            kind: OpSetKind::FillSketch,
            ops,
        }
    }
}

/// rough's `Drawable`: the shape name plus the op sets to paint, in paint order.
///
/// The `options` are deliberately *not* carried here. In rough they ride along so
/// `RoughCanvas` can read `stroke`/`fill`/`strokeWidth` at paint time; our renderer
/// resolves styling from the element instead, and keeping options out means a
/// `Drawable` is pure geometry and therefore cacheable on `(id, version, w, h)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawable {
    pub shape: &'static str,
    pub sets: Vec<OpSet>,
}
