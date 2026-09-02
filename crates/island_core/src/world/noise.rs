//! Deterministic value noise.
//!
//! # Everything here must be bit-identical on every platform
//!
//! A seed has to determine a world. Shareable seeds, saved games, and the
//! browser-versus-native check in the smoke test all depend on it, and if it
//! is ever false it will be false *silently* — surfacing much later as reports
//! of worlds not matching between machines.
//!
//! IEEE-754 pins `+ - * /`, `sqrt`, and `floor` to exactly-rounded results, so
//! those are safe. It says nothing about transcendental functions: `sin`,
//! `cos`, `exp`, `powf` and friends are left to the implementation, and Rust
//! forwards them to the platform's libm. **wasm's libm is not glibc's.** One
//! `sin` in this file is enough to make the browser and the native tests
//! disagree.
//!
//! So this module uses integer hashing and polynomial interpolation only. That
//! constraint is enforced by a test that reads this file's own source, not
//! merely by this comment.
//!
//! Two further hazards, both currently safe but worth knowing:
//!
//! - **Fused multiply-add.** If a compiler contracted `a * b + c` into an FMA
//!   on one target and not another, results would diverge. Rust does not
//!   permit automatic FMA contraction — it only happens via an explicit
//!   `mul_add` — so this is safe as long as nothing here calls it.
//! - **x87 excess precision.** 32-bit x86 evaluates `f32` in 80-bit registers
//!   and can produce different results from SSE. Irrelevant on x86-64 and
//!   wasm; it would matter if a 32-bit x86 target were ever added.

/// Integer bit-mixer with good avalanche.
///
/// This is Wellons' `lowbias32`. Every operation is integer, so it is exact
/// and identical everywhere by construction — which is the entire reason the
/// randomness in this module comes from integers rather than from `sin`, the
/// usual shader-style trick.
const fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Hash a lattice point and seed to a `u32`.
///
/// Three sequential rounds, each folding in one input pre-multiplied by its
/// own large odd constant. The construction looks fussier than it needs to be;
/// each part of it is fixing a real defect found by inspecting the output:
///
/// - **The constants must differ per input.** The obvious
///   `mix(seed ^ x)` then `mix(h ^ z)` makes the seed and the X coordinate
///   *interchangeable* — `hash_2d(1, 0, 0)` and `hash_2d(0, 0, 1)` return the
///   same value. Terrain built on that would make changing the seed equivalent
///   to shifting the world by one tile, so every "different" seed would give
///   the same world moved sideways.
/// - **The non-zero offset matters.** Zero is a fixed point of the mixer:
///   every operation in it maps 0 to 0. Without `SALT`, `hash_2d(0, 0, 0)` is
///   exactly 0, which puts a hard artefact at the origin of the default seed.
///
/// Hashing is not the bottleneck here — roughly 24 calls per tile — so the
/// safer construction is free.
pub const fn hash_2d(x: i32, z: i32, seed: u32) -> u32 {
    /// Arbitrary non-zero constant, so all-zero input is not a fixed point.
    const SALT: u32 = 0x632b_e5ab;

    let h = mix(seed.wrapping_mul(0x9e37_79b1) ^ SALT);
    let h = mix(h ^ (x as u32).wrapping_mul(0x85eb_ca77));
    mix(h ^ (z as u32).wrapping_mul(0xc2b2_ae3d))
}

/// Map a hash to `[0, 1)`.
///
/// Takes the top 24 bits because `f32` has exactly 24 bits of mantissa: the
/// integer-to-float conversion is then exact, and scaling by `2^-24` is exact
/// too, being a power of two. No rounding happens anywhere, so this is
/// reproducible to the bit.
pub fn hash_to_unit(h: u32) -> f32 {
    const SCALE: f32 = 1.0 / 16_777_216.0; // 2^-24
    (h >> 8) as f32 * SCALE
}

/// Perlin's quintic fade, `6t⁵ - 15t⁴ + 10t³`.
///
/// Preferred over the cubic `3t² - 2t³` because its second derivative is zero
/// at both ends. Cubic smoothstep leaves a discontinuity in curvature at every
/// lattice line, which lighting picks up as a faint grid of creases across the
/// terrain — the classic giveaway of a hand-rolled value noise.
///
/// Written as nested multiplication so it is only multiplies and adds.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 2D value noise in `[0, 1)`.
///
/// Samples the four surrounding lattice points and interpolates. At exact
/// integer coordinates the result is the lattice value itself.
pub fn value_noise_2d(x: f32, z: f32, seed: u32) -> f32 {
    // `floor` is exactly specified by IEEE-754 (round toward negative
    // infinity), so it is safe here despite being a float operation.
    let x0 = x.floor();
    let z0 = z.floor();
    let xi = x0 as i32;
    let zi = z0 as i32;

    let fx = fade(x - x0);
    let fz = fade(z - z0);

    let v00 = hash_to_unit(hash_2d(xi, zi, seed));
    let v10 = hash_to_unit(hash_2d(xi + 1, zi, seed));
    let v01 = hash_to_unit(hash_2d(xi, zi + 1, seed));
    let v11 = hash_to_unit(hash_2d(xi + 1, zi + 1, seed));

    lerp(lerp(v00, v10, fx), lerp(v01, v11, fx), fz)
}

/// Fractional Brownian motion: value noise summed over several octaves.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Fbm {
    /// How many octaves to sum. Each costs four hashes per sample.
    pub octaves: u32,
    /// Frequency of the first octave, in cycles per world unit. Smaller means
    /// larger features.
    pub frequency: f32,
    /// Frequency multiplier between octaves. 2.0 is the usual choice.
    pub lacunarity: f32,
    /// Amplitude multiplier between octaves. Below 0.5 gives smooth terrain,
    /// above it gives rough.
    pub gain: f32,
}

impl Default for Fbm {
    fn default() -> Self {
        Self {
            octaves: 5,
            frequency: 1.0 / 64.0,
            lacunarity: 2.0,
            gain: 0.5,
        }
    }
}

impl Fbm {
    /// Sample at a world position. Result is in `[0, 1)`.
    ///
    /// Normalised by the sum of amplitudes rather than by an assumed maximum,
    /// so changing `gain` or `octaves` does not silently change the output
    /// range and shift every elevation band with it.
    pub fn sample(&self, x: f32, z: f32, seed: u32) -> f32 {
        let mut frequency = self.frequency;
        let mut amplitude = 1.0f32;
        let mut sum = 0.0f32;
        let mut total_amplitude = 0.0f32;

        for octave in 0..self.octaves {
            // A distinct seed per octave. Reusing one seed makes every octave
            // the same pattern at a different scale, which reads as an
            // obviously repeating texture rather than as terrain.
            let octave_seed = mix(seed ^ octave.wrapping_mul(0x9e37_79b9));
            sum += value_noise_2d(x * frequency, z * frequency, octave_seed) * amplitude;
            total_amplitude += amplitude;

            frequency *= self.lacunarity;
            amplitude *= self.gain;
        }

        if total_amplitude > 0.0 {
            sum / total_amplitude
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Regressions for two defects found by reading the output ----

    /// Zero is a fixed point of the mixer: every operation in it maps 0 to 0.
    /// Without a non-zero salt, the default seed had a hard artefact at the
    /// world origin.
    #[test]
    fn all_zero_input_does_not_hash_to_zero() {
        // The fixed point is real and is a property of the mixer, not a bug
        // in it: xor-shift and multiply both map 0 to 0. Asserted so the
        // reason `hash_2d` needs a salt stays visible.
        assert_eq!(
            mix(0),
            0,
            "mix has a fixed point at zero; that is why SALT exists"
        );

        // The salt is what keeps it out of the generated world.
        assert_ne!(hash_2d(0, 0, 0), 0);
    }

    /// The seed must not be interchangeable with a coordinate.
    ///
    /// The natural `mix(seed ^ x)` construction made `hash_2d(1, 0, 0)` and
    /// `hash_2d(0, 0, 1)` identical, which would have made every "different"
    /// seed produce the same world shifted by a tile.
    #[test]
    fn seed_and_coordinate_are_not_interchangeable() {
        assert_ne!(hash_2d(1, 0, 0), hash_2d(0, 0, 1));
        assert_ne!(hash_2d(0, 1, 0), hash_2d(0, 0, 1));
        assert_ne!(hash_2d(5, 0, 3), hash_2d(3, 0, 5));
    }

    /// The stronger form of the above, on the actual noise field rather than
    /// the hash: a new seed must give a *different world*, not the same world
    /// translated. Checks every small offset, since a one-tile shift is
    /// exactly what the original bug produced.
    #[test]
    fn changing_the_seed_does_not_merely_translate_the_world() {
        const N: i32 = 24;
        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)] {
            let mut matches = 0;
            let mut total = 0;
            for z in 0..N {
                for x in 0..N {
                    let a = value_noise_2d(x as f32 * 0.5, z as f32 * 0.5, 1);
                    let b = value_noise_2d((x + dx) as f32 * 0.5, (z + dz) as f32 * 0.5, 2);
                    if a.to_bits() == b.to_bits() {
                        matches += 1;
                    }
                    total += 1;
                }
            }
            assert!(
                matches * 10 < total,
                "seed 2 looks like seed 1 shifted by ({dx}, {dz}): {matches}/{total} samples equal"
            );
        }
    }

    // ---- Determinism, the reason this module exists ----

    /// Bit-identical, not approximately equal. Approximate equality would let
    /// a real platform divergence through.
    #[test]
    fn same_seed_and_position_are_bit_identical() {
        for i in 0..200 {
            let (x, z) = (i as f32 * 0.37, i as f32 * -0.61);
            assert_eq!(
                value_noise_2d(x, z, 99).to_bits(),
                value_noise_2d(x, z, 99).to_bits()
            );
            let f = Fbm::default();
            assert_eq!(f.sample(x, z, 99).to_bits(), f.sample(x, z, 99).to_bits());
        }
    }

    /// Baked from the implementation, so this detects *change*, not
    /// correctness — correctness is what the range, continuity and
    /// interchangeability tests above are for.
    ///
    /// Its real job is cross-platform: these same values are asserted from the
    /// browser via `world_hash`, so a divergence between wasm and native shows
    /// up here rather than as worlds that quietly fail to match between
    /// machines.
    #[test]
    fn golden_values_are_unchanged() {
        let hashes = [
            ((0, 0, 0u32), 0x8a06_eb4a_u32),
            ((1, 0, 0), 0x878e_2468),
            ((0, 1, 0), 0x26f8_97ca),
            ((-1, -1, 0), 0x833a_23c7),
            ((0, 0, 1), 0xb37d_7630),
            ((12345, -6789, 42), 0x08ab_1d51),
        ];
        for ((x, z, seed), want) in hashes {
            assert_eq!(hash_2d(x, z, seed), want, "hash_2d({x}, {z}, {seed})");
        }

        let noise = [
            ((0.0f32, 0.0f32), 0.004_429_102_f32),
            ((0.5, 0.5), 0.506_906_45),
            ((1.25, -3.75), 0.452_824_1),
            ((-0.5, 2.0), 0.872_844_2),
            ((100.125, 200.875), 0.125_729_86),
        ];
        for ((x, z), want) in noise {
            assert_eq!(
                value_noise_2d(x, z, 1337).to_bits(),
                want.to_bits(),
                "value_noise_2d({x}, {z}, 1337) = {}",
                value_noise_2d(x, z, 1337)
            );
        }

        let fbm = [
            ((0.0f32, 0.0f32), 0.744_138_96_f32),
            ((10.5, -20.25), 0.727_533_46),
            ((128.0, 128.0), 0.820_429),
            ((-64.5, 64.5), 0.508_689_3),
        ];
        let f = Fbm::default();
        for ((x, z), want) in fbm {
            assert_eq!(
                f.sample(x, z, 20_260_902).to_bits(),
                want.to_bits(),
                "Fbm::default().sample({x}, {z}, 20260902) = {}",
                f.sample(x, z, 20_260_902)
            );
        }
    }

    /// The constraint from `issues.md` §2, enforced against this file's own
    /// source rather than trusted to a comment.
    ///
    /// IEEE-754 pins arithmetic exactly but leaves transcendentals to the
    /// platform libm, and wasm's is not glibc's. Scans only the code above the
    /// test module, since the forbidden names necessarily appear below.
    #[test]
    fn source_calls_no_transcendental_functions() {
        let source = include_str!("noise.rs");
        let code = &source[..source.find("#[cfg(test)]").expect("test module marker")];

        // Call syntax, not bare words: the doc comments discuss `sin` and
        // `powf` by name and must not trip this.
        const FORBIDDEN: [&str; 18] = [
            ".sin(", ".cos(", ".tan(", ".asin(", ".acos(", ".atan(", ".atan2(", ".sinh(", ".cosh(",
            ".tanh(", ".exp(", ".exp2(", ".ln(", ".log(", ".log2(", ".log10(", ".powf(", ".powi(",
        ];
        for name in FORBIDDEN {
            assert!(
                !code.contains(name),
                "{name} is not exactly specified by IEEE-754 and may differ \
                 between wasm and native, which would make the same seed \
                 generate different worlds. See the module docs."
            );
        }
        // `floor`, `sqrt` and `mul_add` are exactly specified and would be
        // safe; they are simply unused. `.floor()` is used and is fine.
        assert!(
            code.contains(".floor()"),
            "floor is expected to be used here"
        );
    }

    // ---- Value properties ----

    #[test]
    fn hash_to_unit_stays_in_unit_range() {
        for h in [0u32, 1, 0x7fff_ffff, 0x8000_0000, u32::MAX] {
            let v = hash_to_unit(h);
            assert!((0.0..1.0).contains(&v), "hash_to_unit({h:#x}) = {v}");
        }
        assert_eq!(hash_to_unit(0).to_bits(), 0.0f32.to_bits());
        assert!(hash_to_unit(u32::MAX) < 1.0);
    }

    #[test]
    fn noise_stays_in_unit_range_and_is_never_nan() {
        let f = Fbm::default();
        for i in -500..500 {
            let (x, z) = (i as f32 * 1.7, i as f32 * -2.3);
            let v = value_noise_2d(x, z, 7);
            assert!(
                v.is_finite() && (0.0..1.0).contains(&v),
                "value_noise at {x},{z} = {v}"
            );
            let s = f.sample(x, z, 7);
            assert!(
                s.is_finite() && (0.0..1.0).contains(&s),
                "fbm at {x},{z} = {s}"
            );
        }
    }

    /// At integer coordinates the interpolation weights are 0, so the result
    /// must be exactly the lattice value. A mismatch means the fade or the
    /// floor is off by one.
    #[test]
    fn lattice_points_return_their_lattice_value() {
        for z in -3..3 {
            for x in -3..3 {
                let expected = hash_to_unit(hash_2d(x, z, 55));
                let actual = value_noise_2d(x as f32, z as f32, 55);
                assert_eq!(actual.to_bits(), expected.to_bits(), "at ({x}, {z})");
            }
        }
    }

    #[test]
    fn fade_is_zero_and_one_at_the_ends() {
        assert_eq!(fade(0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(fade(1.0).to_bits(), 1.0f32.to_bits());
        // Symmetric about the midpoint.
        assert!((fade(0.5) - 0.5).abs() < 1e-6);
    }

    /// Nearby samples must be close, or the terrain is noise rather than
    /// landscape. Catches a fade or interpolation error that the range tests
    /// would not.
    #[test]
    fn noise_is_continuous() {
        const STEP: f32 = 0.01;
        for i in 0..400 {
            let x = i as f32 * 0.13;
            let a = value_noise_2d(x, 3.7, 11);
            let b = value_noise_2d(x + STEP, 3.7, 11);
            assert!(
                (a - b).abs() < 0.1,
                "jump of {} between x={x} and x={}",
                (a - b).abs(),
                x + STEP
            );
        }
    }

    // ---- fBm behaviour ----

    /// Normalising by the summed amplitude means the output range does not
    /// depend on octave count or gain. Without it, adding an octave would
    /// shift every elevation band and silently rewrite the world.
    #[test]
    fn fbm_range_is_independent_of_octaves_and_gain() {
        for octaves in 1..=8 {
            for gain in [0.25f32, 0.5, 0.75] {
                let f = Fbm {
                    octaves,
                    gain,
                    ..Fbm::default()
                };
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for i in 0..300 {
                    let v = f.sample(i as f32 * 3.1, i as f32 * -1.9, 4);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                assert!(
                    lo >= 0.0 && hi < 1.0,
                    "octaves={octaves} gain={gain}: [{lo}, {hi}]"
                );
                assert!(
                    hi - lo > 0.15,
                    "octaves={octaves} gain={gain}: range collapsed to {}",
                    hi - lo
                );
            }
        }
    }

    #[test]
    fn fbm_with_one_octave_matches_plain_noise() {
        let f = Fbm {
            octaves: 1,
            frequency: 1.0,
            ..Fbm::default()
        };
        // One octave normalises by its own amplitude of 1.0, so it should be
        // the underlying noise at the octave's derived seed.
        let seed = mix(3u32);
        assert_eq!(
            f.sample(2.5, -1.5, 3).to_bits(),
            value_noise_2d(2.5, -1.5, seed).to_bits()
        );
    }

    #[test]
    fn zero_octaves_is_defined_rather_than_nan() {
        let f = Fbm {
            octaves: 0,
            ..Fbm::default()
        };
        assert_eq!(f.sample(1.0, 1.0, 1).to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn different_seeds_give_different_fields() {
        let f = Fbm::default();
        let differing = (0..200)
            .filter(|i| {
                let (x, z) = (*i as f32 * 2.3, *i as f32 * 1.1);
                f.sample(x, z, 1).to_bits() != f.sample(x, z, 2).to_bits()
            })
            .count();
        assert!(
            differing > 190,
            "only {differing}/200 samples differed between seeds"
        );
    }
}
