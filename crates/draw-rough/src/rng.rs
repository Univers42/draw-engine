//! The rough.js pseudo-random generator.
//!
//! Transcribed from roughjs 4.6.4 `bin/math.js` — the exact version Excalidraw pins
//! (`packages/excalidraw/package.json`, no caret):
//!
//! ```js
//! export class Random {
//!     constructor(seed) { this.seed = seed; }
//!     next() {
//!         if (this.seed) {
//!             return ((2 ** 31 - 1) & (this.seed = Math.imul(48271, this.seed))) / 2 ** 31;
//!         } else {
//!             return Math.random();
//!         }
//!     }
//! }
//! ```
//!
//! Three details each change the output stream, and all three are easy to get wrong:
//!
//! 1. `Math.imul` is a 32-bit **signed** wrapping multiply, so the state goes negative
//!    roughly half the time. It is *not* `seed * 48271 % 2147483647` — that is a
//!    different generator, and assuming it silently corrupts every coordinate.
//! 2. `& (2 ** 31 - 1)` clears the sign bit, yielding `0 ..= 2^31 - 1`.
//! 3. The divisor is `2 ** 31`, **not** `2^31 - 1`.

/// rough.js's `Random`, reproducing `Math.imul(48271, seed)` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Random {
    seed: i32,
}

impl Random {
    /// Seeds the generator. Takes `i32` because that is what `Math.imul` coerces its
    /// operand to; a `u32` seed from the element model converts with `as i32`, which
    /// performs the same wrap ECMAScript's ToInt32 would.
    pub const fn new(seed: i32) -> Self {
        Self { seed }
    }

    /// The next value in `[0, 1)`.
    ///
    /// On a zero seed rough falls through to `Math.random()`, which is not reproducible
    /// — not by us, and not by rough itself across two runs. We return `0.0` so our
    /// output is at least deterministic. Excalidraw draws seeds from `randomInteger()`,
    /// so this is reachable only for the single seed value 0.
    ///
    /// Named `next` because rough.js names it `next`. Clippy reads that as shadowing
    /// `Iterator::next` and suggests renaming; in a line-by-line transcription the
    /// matching name is worth more than the lint, since the whole file is audited
    /// against the original. It is not an iterator — it never ends.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        if self.seed == 0 {
            return 0.0;
        }
        self.seed = 48271i32.wrapping_mul(self.seed);
        f64::from(self.seed & 0x7FFF_FFFF) / 2_147_483_648.0
    }
}

#[cfg(test)]
mod tests {
    use super::Random;

    /// Ground truth captured from roughjs 4.6.4 itself:
    ///
    /// ```sh
    /// node -e 'import("./package/bin/math.js").then(({Random}) => {
    ///   const r = new Random(SEED);
    ///   console.log(Array.from({length: 6}, () => r.next()).map(v => v.toExponential(17)));
    /// })'
    /// ```
    ///
    /// 18 significant digits round-trips an f64 exactly, and Rust parses decimal
    /// literals correctly-rounded, so these compare with `==` rather than a tolerance.
    /// The PRNG is the one place in this crate where exactness is not merely a goal.
    const REFERENCE: &[(i32, [f64; 6])] = &[
        (
            1,
            [
                2.24779359996318817e-5,
                8.50324486382305622e-2,
                6.01328216027468443e-1,
                7.14315861929208040e-1,
                7.40971184801310301e-1,
                4.20061544049531221e-1,
            ],
        ),
        (
            12345,
            [
                2.77490119915455580e-1,
                7.25578438956290483e-1,
                3.96826859097927809e-1,
                2.29315516073256731e-1,
                2.89276372175663710e-1,
                6.59761291462928057e-1,
            ],
        ),
        (
            i32::MAX,
            [
                9.99977522064000368e-1,
                9.14967551361769438e-1,
                3.98671783972531557e-1,
                2.85684138070791960e-1,
                2.59028815198689699e-1,
                5.79938455950468779e-1,
            ],
        ),
        // A negative seed is reachable: `if (this.seed)` is truthy for it, and the
        // state reaches negative values on its own after the first multiply anyway.
        (
            -7,
            [
                9.99842654448002577e-1,
                4.04772859532386065e-1,
                7.90702487807720900e-1,
                9.99788966495543718e-1,
                8.13201706390827894e-1,
                5.95691916532814503e-2,
            ],
        ),
        (
            987654321,
            [
                4.30617197882384062e-1,
                3.22758980561047792e-1,
                8.98750662337988615e-1,
                5.93221717048436403e-1,
                4.05503645073622465e-1,
                6.64513488300144672e-2,
            ],
        ),
    ];

    #[test]
    fn reproduces_roughjs_bit_for_bit() {
        for (seed, want) in REFERENCE {
            let mut r = Random::new(*seed);
            for (i, expected) in want.iter().enumerate() {
                let got = r.next();
                assert_eq!(
                    got.to_bits(),
                    expected.to_bits(),
                    "seed {seed}, draw {i}: got {got:e}, want {expected:e}"
                );
            }
        }
    }

    #[test]
    fn zero_seed_is_deterministic_rather_than_random() {
        let mut r = Random::new(0);
        assert_eq!(r.next(), 0.0);
        assert_eq!(r.next(), 0.0);
    }

    /// The state goes negative, but the returned value must never leave `[0, 1)`.
    #[test]
    fn output_stays_in_unit_interval() {
        let mut r = Random::new(i32::MAX);
        for _ in 0..100_000 {
            let v = r.next();
            assert!((0.0..1.0).contains(&v), "out of range: {v}");
        }
    }

    /// A u32 seed from the element model must wrap the way ECMAScript's ToInt32 does.
    #[test]
    fn u32_seed_wraps_like_to_int32() {
        let big: u32 = 4_294_967_289; // 2^32 - 7  ->  -7 as i32
        assert_eq!(Random::new(big as i32), Random::new(-7));
    }
}
