//! Tiny pure math helpers. No I/O, no host.

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn smoothstep(t: f64) -> f64 {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// JS-compatible string hash (`Math.imul(31, hash) + codeUnit`).
pub fn hash_string(value: &str) -> u32 {
    let mut hash: i32 = 0;
    for unit in value.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash.unsigned_abs()
}

pub fn round_px(value: f64) -> f64 {
    value.round()
}
