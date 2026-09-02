//! Turning a seed into a world.
//!
//! Generation is one pure call: [`WorldParams::generate`] takes everything it
//! needs and returns a [`TileMap`]. No global state, no hidden RNG, no order
//! dependence — the same params always produce the same world, on any
//! platform. See [`super::noise`] for why the last part is not free.

use super::noise::Fbm;
use super::{TileKind, TileMap, TilePos};

/// Height steps above sea level that are beach rather than grass.
///
/// One step, so the shoreline is a thin margin rather than a wide desert.
const BEACH_STEPS: i16 = 1;

/// Percentage of the land height range (above the beach) that is grass.
const GRASS_PERCENT: i32 = 45;

/// Percentage of the land height range that is grass-or-rock. Above this is
/// snow, so rock occupies the band between `GRASS_PERCENT` and this.
const ROCK_PERCENT: i32 = 80;

/// Everything needed to generate a world.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WorldParams {
    pub seed: u32,
    /// World size in chunks. Tiles are derived, so the world cannot be
    /// chunk-misaligned.
    pub chunks_x: u32,
    pub chunks_z: u32,
    /// Height the noise floor maps to, in steps.
    ///
    /// A *theoretical* bound, not an observed one: fBm is a sum of octaves
    /// normalised to `[0, 1)`, and reaching either extreme needs every octave
    /// to agree, which effectively never happens. Real terrain occupies a
    /// noticeably narrower band than `min_height..=max_height` — which is why
    /// the land material bands are scaled from the observed range instead of
    /// from these. See `issues.md` §15.
    pub min_height: i16,
    /// Height the noise ceiling maps to, in steps. See `min_height`.
    pub max_height: i16,
    /// Tiles at or below this height are water.
    pub sea_level: i16,
    pub terrain: Fbm,
}

impl Default for WorldParams {
    fn default() -> Self {
        Self {
            seed: 1,
            chunks_x: 8,
            chunks_z: 8,
            min_height: -8,
            max_height: 24,
            sea_level: 6,
            terrain: Fbm::default(),
        }
    }
}

impl WorldParams {
    /// Total height span in steps, at least 1 so the mapping never divides by
    /// zero on a degenerate flat world.
    fn height_span(&self) -> i16 {
        (self.max_height - self.min_height).max(1)
    }

    /// Generate the world. Pure: same params in, same world out.
    ///
    /// Two passes. The first lays down heights; the second classifies them,
    /// which needs the world's observed peak and so cannot be done until every
    /// height exists. Still deterministic — the peak is a function of the
    /// heights, which are a function of the params.
    pub fn generate(&self) -> TileMap {
        let mut map = TileMap::new(self.chunks_x, self.chunks_z);
        let span = self.height_span() as f32;

        for z in 0..map.depth() as i32 {
            for x in 0..map.width() as i32 {
                // Sample at the tile's centre rather than its corner, so a
                // tile's height is not biased toward one side of itself.
                let n = self
                    .terrain
                    .sample(x as f32 + 0.5, z as f32 + 0.5, self.seed);
                map.set_height(TilePos::new(x, z), self.height_from_noise(n, span));
            }
        }

        let land_top = self.observed_land_top(&map);

        for z in 0..map.depth() as i32 {
            for x in 0..map.width() as i32 {
                let pos = TilePos::new(x, z);
                let height = map.height(pos).expect("in bounds");
                map.set_kind(pos, self.classify(height, land_top));
            }
        }

        map
    }

    /// The highest point in the world, floored at the beach top.
    ///
    /// The land bands are scaled from this rather than from `max_height`,
    /// because fBm never reaches its nominal ceiling: with the defaults the
    /// terrain topped out at 18 against a `max_height` of 24, which put the
    /// snow band above the highest ground in the world and produced exactly
    /// zero snow tiles.
    fn observed_land_top(&self, map: &TileMap) -> i16 {
        let beach_top = self.sea_level.saturating_add(BEACH_STEPS);
        map.heights()
            .iter()
            .copied()
            .max()
            .unwrap_or(beach_top)
            .max(beach_top.saturating_add(1))
    }

    /// Map noise in `[0, 1)` onto `[min_height, max_height]`.
    ///
    /// Rounds with `floor(v + 0.5)` rather than `round()`. Both are almost
    /// certainly fine, but `floor` and addition are exactly specified by
    /// IEEE-754 whereas `round`'s ties-away behaviour has to be synthesised on
    /// some targets — and this module's whole contract is that wasm and native
    /// agree to the bit. Not worth the doubt for the sake of one call.
    fn height_from_noise(&self, noise: f32, span: f32) -> i16 {
        let steps = (noise * span + 0.5).floor();
        // Float-to-int `as` saturates in Rust rather than being UB, so a
        // pathological span cannot produce a wrapped height.
        let height = self.min_height.saturating_add(steps as i16);

        // Clamp so "heights lie in min..=max" holds unconditionally, including
        // when the two are equal (the span floors at 1, which would otherwise
        // let a height sit one step above max) or supplied inverted. Ordered
        // explicitly because `clamp` panics when its bounds are reversed.
        let lo = self.min_height.min(self.max_height);
        let hi = self.min_height.max(self.max_height);
        height.clamp(lo, hi)
    }

    /// Which material a tile of this height is.
    ///
    /// Water and beach are **absolute** — sea level is a real elevation and
    /// does not move with the terrain. The land bands above them are
    /// **relative** to `land_top`, the world's observed peak, so the highest
    /// ground is always snow-capped whatever the noise happened to produce.
    ///
    /// Boundaries are integer arithmetic throughout: no float comparison, and
    /// therefore nothing that could differ between platforms.
    pub fn classify(&self, height: i16, land_top: i16) -> TileKind {
        if height <= self.sea_level {
            return TileKind::Water;
        }

        let beach_top = self.sea_level.saturating_add(BEACH_STEPS);
        if height <= beach_top {
            return TileKind::Sand;
        }

        let land_span = (land_top - beach_top).max(1) as i32;
        let above_beach = (height - beach_top) as i32;

        if above_beach * 100 <= land_span * GRASS_PERCENT {
            TileKind::Grass
        } else if above_beach * 100 <= land_span * ROCK_PERCENT {
            TileKind::Rock
        } else {
            TileKind::Snow
        }
    }
}

/// A checksum over a world's full contents.
///
/// Exists to answer one question: does the browser generate the *same* world
/// as the native test run? The no-transcendentals rule in [`super::noise`] is
/// reasoning; this is what makes it testable. Without it, a wasm/native
/// divergence would be silent.
///
/// FNV-1a, which is order-*dependent* on purpose — two worlds holding the same
/// multiset of tiles in different arrangements must not collide. Dimensions
/// are folded in first so an all-water 64×96 world differs from an all-water
/// 96×64 one.
pub fn world_hash(map: &TileMap) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    let mut feed = |byte: u8| {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    };

    for byte in map.width().to_le_bytes() {
        feed(byte);
    }
    for byte in map.depth().to_le_bytes() {
        feed(byte);
    }
    for height in map.heights() {
        for byte in height.to_le_bytes() {
            feed(byte);
        }
    }
    for kind in map.kinds() {
        feed(*kind as u8);
    }

    hash
}

#[cfg(test)]
mod inspect {
    use super::*;
    use crate::world::TilePos;

    /// Prints what generation actually produces, so the shape of the world can
    /// be judged before any of it is baked into a golden test.
    /// `cargo test -p island_core -- --ignored --nocapture inspect_world`
    #[test]
    #[ignore = "diagnostic, not an assertion"]
    fn inspect_world() {
        let params = WorldParams::default();
        let map = params.generate();

        let (mut lo, mut hi) = (i16::MAX, i16::MIN);
        let mut counts = [0usize; 5];
        for (i, h) in map.heights().iter().enumerate() {
            lo = lo.min(*h);
            hi = hi.max(*h);
            counts[map.kinds()[i] as usize] += 1;
        }
        let total = map.tile_count() as f32;

        println!(
            "size      {}x{} ({} tiles)",
            map.width(),
            map.depth(),
            map.tile_count()
        );
        println!(
            "params    min={} max={} sea={}",
            params.min_height, params.max_height, params.sea_level
        );
        println!("heights   actual range {lo}..={hi}");
        for kind in TileKind::ALL {
            let n = counts[kind as usize];
            println!(
                "  {:<6} {:>7}  {:>5.1}%",
                kind.name(),
                n,
                n as f32 / total * 100.0
            );
        }
        println!("hash      0x{:016x}", world_hash(&map));

        // A coarse picture, so the terrain can be eyeballed as terrain.
        println!("\n--- 64x32 sample, every 4th tile ---");
        for z in 0..32 {
            let row: String = (0..64)
                .map(|x| match map.kind(TilePos::new(x * 4, z * 4)).unwrap() {
                    TileKind::Water => '~',
                    TileKind::Sand => '.',
                    TileKind::Grass => '#',
                    TileKind::Rock => '^',
                    TileKind::Snow => '*',
                })
                .collect();
            println!("{row}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{ChunkPos, CHUNK_SIZE};

    /// Small enough to generate quickly in tests that run many worlds.
    fn small() -> WorldParams {
        WorldParams {
            chunks_x: 2,
            chunks_z: 2,
            ..WorldParams::default()
        }
    }

    // ---- Determinism: the property the whole module exists to guarantee ----

    #[test]
    fn same_params_produce_the_same_world() {
        let p = small();
        assert_eq!(world_hash(&p.generate()), world_hash(&p.generate()));
    }

    #[test]
    fn different_seeds_produce_different_worlds() {
        let a = WorldParams { seed: 1, ..small() };
        let b = WorldParams { seed: 2, ..small() };
        assert_ne!(world_hash(&a.generate()), world_hash(&b.generate()));
    }

    /// Generation must not depend on what was generated before it. Catches any
    /// accidental shared or cached state.
    #[test]
    fn generation_order_does_not_matter() {
        let a = WorldParams {
            seed: 10,
            ..small()
        };
        let b = WorldParams {
            seed: 20,
            ..small()
        };

        let a_first = world_hash(&a.generate());
        let _ = b.generate();
        let a_after_b = world_hash(&a.generate());

        assert_eq!(a_first, a_after_b);
    }

    /// The value the browser is asserted against in `G3`. A change here means
    /// either a deliberate generation change — in which case update it — or a
    /// platform divergence, which is the whole point of having it.
    #[test]
    fn golden_world_hash_is_unchanged() {
        let map = WorldParams::default().generate();
        assert_eq!(
            world_hash(&map),
            0xd34b_fa9b_078f_3806,
            "default world hash changed: 0x{:016x}",
            world_hash(&map)
        );
    }

    // ---- world_hash behaviour ----

    #[test]
    fn hash_changes_when_a_single_tile_changes() {
        let p = small();
        let map = p.generate();
        let before = world_hash(&map);

        let mut changed = p.generate();
        let pos = TilePos::new(3, 4);
        let h = changed.height(pos).unwrap();
        changed.set_height(pos, h + 1);
        assert_ne!(
            world_hash(&changed),
            before,
            "one height change must alter the hash"
        );

        let mut changed = p.generate();
        let k = changed.kind(pos).unwrap();
        let other = if k == TileKind::Snow {
            TileKind::Water
        } else {
            TileKind::Snow
        };
        changed.set_kind(pos, other);
        assert_ne!(
            world_hash(&changed),
            before,
            "one kind change must alter the hash"
        );
    }

    /// Two worlds holding identical tile data in different shapes must not
    /// collide, which is why the dimensions are folded in first.
    #[test]
    fn hash_distinguishes_worlds_of_different_shape() {
        let wide = TileMap::new(2, 1);
        let tall = TileMap::new(1, 2);
        // Same tile count, same contents (both fresh ocean), different shape.
        assert_eq!(wide.tile_count(), tall.tile_count());
        assert_ne!(world_hash(&wide), world_hash(&tall));
    }

    // ---- Height range ----

    #[test]
    fn heights_stay_within_the_configured_range() {
        for seed in 0..8u32 {
            let p = WorldParams { seed, ..small() };
            let map = p.generate();
            for h in map.heights() {
                assert!(
                    *h >= p.min_height && *h <= p.max_height,
                    "height {h} outside {}..={}",
                    p.min_height,
                    p.max_height
                );
            }
        }
    }

    /// The degenerate case that motivated clamping: with min == max the span
    /// floors at 1, which without the clamp lets a height land one step above
    /// max.
    #[test]
    fn equal_min_and_max_gives_a_flat_world() {
        let p = WorldParams {
            min_height: 5,
            max_height: 5,
            ..small()
        };
        let map = p.generate();
        assert!(map.heights().iter().all(|h| *h == 5));
    }

    #[test]
    fn inverted_min_and_max_does_not_panic_or_escape_the_range() {
        let p = WorldParams {
            min_height: 10,
            max_height: -10,
            ..small()
        };
        let map = p.generate();
        assert!(map.heights().iter().all(|h| (-10..=10).contains(h)));
    }

    // ---- Classification ----

    #[test]
    fn sea_level_and_below_is_water_and_above_is_not() {
        let p = small();
        let map = p.generate();
        for (i, h) in map.heights().iter().enumerate() {
            let kind = map.kinds()[i];
            if *h <= p.sea_level {
                assert_eq!(
                    kind,
                    TileKind::Water,
                    "height {h} at or below sea level {} should be water",
                    p.sea_level
                );
            } else {
                assert_ne!(
                    kind,
                    TileKind::Water,
                    "height {h} above sea level {} should not be water",
                    p.sea_level
                );
            }
        }
    }

    #[test]
    fn classify_boundaries_are_where_they_claim_to_be() {
        let p = WorldParams {
            sea_level: 6,
            ..small()
        };
        let land_top = 30;
        assert_eq!(
            p.classify(6, land_top),
            TileKind::Water,
            "exactly sea level is water"
        );
        assert_eq!(p.classify(5, land_top), TileKind::Water);
        assert_eq!(
            p.classify(7, land_top),
            TileKind::Sand,
            "one step above sea level is beach"
        );
        assert_eq!(
            p.classify(8, land_top),
            TileKind::Grass,
            "past the beach is grass"
        );
        assert_eq!(
            p.classify(land_top, land_top),
            TileKind::Snow,
            "the peak is snow"
        );
    }

    /// Materials must never go backwards as elevation rises — grass above
    /// rock would look like a generation fault.
    #[test]
    fn classification_is_monotonic_in_height() {
        let p = small();
        let land_top = 40;
        let mut previous = TileKind::Water;
        for h in -20..=land_top {
            let kind = p.classify(h, land_top);
            assert!(
                kind >= previous,
                "material went backwards at height {h}: {} after {}",
                kind.name(),
                previous.name()
            );
            previous = kind;
        }
    }

    /// The defect that made this two-pass: bands scaled from `max_height`
    /// rather than the observed peak put snow above the highest ground in the
    /// world, so no tile ever qualified.
    #[test]
    fn every_material_appears_in_a_default_world() {
        let map = WorldParams::default().generate();
        for kind in TileKind::ALL {
            let count = map.kinds().iter().filter(|k| **k == kind).count();
            assert!(count > 0, "no {} tiles were generated at all", kind.name());
        }
    }

    /// A world whose peaks barely clear the water still gets snow on its
    /// highest ground, because the land bands are relative.
    #[test]
    fn land_bands_scale_to_a_low_relief_world() {
        let p = WorldParams {
            min_height: 0,
            max_height: 10,
            sea_level: 4,
            ..small()
        };
        let map = p.generate();
        assert!(map.kinds().contains(&TileKind::Snow));
        assert!(map.kinds().contains(&TileKind::Water));
    }

    // ---- Shape ----

    #[test]
    fn generated_world_matches_the_requested_chunk_count() {
        let p = WorldParams {
            chunks_x: 3,
            chunks_z: 5,
            ..small()
        };
        let map = p.generate();
        assert_eq!(map.width(), 3 * CHUNK_SIZE);
        assert_eq!(map.depth(), 5 * CHUNK_SIZE);
        assert_eq!(map.chunk_count(), 15);
        assert!(map.chunk_bounds(ChunkPos::new(2, 4)).is_some());
    }

    /// Terrain must actually vary within a chunk, or the noise frequency is
    /// so low that meshing would produce nothing but flat plates.
    #[test]
    fn terrain_varies_within_a_single_chunk() {
        let map = WorldParams::default().generate();
        let heights: Vec<i16> = map
            .chunk_tiles(ChunkPos::new(1, 1))
            .map(|t| map.height(t).unwrap())
            .collect();
        let lo = *heights.iter().min().unwrap();
        let hi = *heights.iter().max().unwrap();
        assert!(
            hi > lo,
            "chunk (1,1) is perfectly flat; check the noise frequency"
        );
    }
}
