//! The tile world: storage, addressing, and chunk decomposition.
//!
//! Conventions here follow `docs/architecture/coordinates.md`. In short: X
//! right, Y up, Z toward the viewer; one tile is `1.0` world unit; a tile's
//! origin is its horizontal centre at ground height.

mod generate;
mod mesh;
mod noise;
mod tile;

pub use generate::{world_hash, WorldParams};
pub use mesh::{mesh_chunk, mesh_world, ChunkMesh, Vertex};
pub use noise::{hash_2d, hash_to_unit, value_noise_2d, Fbm};
pub use tile::TileKind;

use glam::Vec3;

/// Tiles per chunk edge. A chunk is `CHUNK_SIZE × CHUNK_SIZE` tiles.
///
/// The chunk is the unit of both mesh rebuild and frustum culling, which pull
/// in opposite directions: smaller chunks mean cheaper rebuilds when one tile
/// changes but more draw calls, larger chunks the reverse. 32 is a starting
/// point to be measured, not a considered answer.
pub const CHUNK_SIZE: u32 = 32;

/// World units of elevation per unit of [`TileMap::height`].
///
/// Heights are stored as integers — "steps" — and this converts them to world
/// Y. Keeping the two separate means the vertical scale can be retuned without
/// touching stored data, and makes it unambiguous whether a given `5` means
/// five steps or five world units.
///
/// **Chosen from rendered comparisons at 1.0, 2.0 and 3.0** (unit `0002`,
/// group F), not from a table:
///
/// - `1.0` — a step is as tall as a tile is wide. Reads as a coloured *map*.
///   Cliff faces are present and correctly lit but too shallow to see, so the
///   terrain looks flat despite being fully three-dimensional.
/// - `2.0` — reads as terraced landscape. Cliffs are unmistakable, mountain
///   shapes are legible, and almost nothing is hidden behind them.
/// - `3.0` — dramatic, but cliffs grow tall enough to **occlude the ground
///   behind them**, which is a genuine problem for a top-down game where the
///   hidden ground is playable space.
///
/// # Gameplay consequence, deliberately accepted
///
/// At 2.0 a terrace is two world units — taller than a person. Every height
/// change is therefore a **barrier** rather than a step up, so movement has to
/// route around cliffs or find a slope. That suits a survival map (natural
/// boundaries, defensible ground) but it is a design commitment, not just a
/// visual one.
pub const HEIGHT_STEP: f32 = 2.0;

/// A tile's integer position on the ground plane.
///
/// Signed, because neighbour queries at the world edge legitimately produce
/// `-1`. Out-of-bounds positions are representable on purpose; the map returns
/// `None` for them rather than making the caller pre-check.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TilePos {
    pub x: i32,
    pub z: i32,
}

/// A chunk's integer position in the chunk grid.
///
/// A separate type from [`TilePos`] on purpose. Both are a pair of integers
/// addressing the ground plane, and passing one where the other is expected is
/// a bug that produces plausible-looking output 32 tiles from where it should
/// be. The compiler catches it instead.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl TilePos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// The chunk containing this tile.
    ///
    /// Uses `div_euclid`, not `/`. Rust's `/` truncates toward zero, so
    /// `-1 / 32 == 0` — which would put tile `-1` in chunk `0` alongside tile
    /// `0`. `div_euclid` floors, giving `-1`. The finite world starts at the
    /// origin so negatives are always out of bounds today, but getting this
    /// wrong now would be an ugly surprise if the world ever streams.
    pub const fn chunk(self) -> ChunkPos {
        ChunkPos {
            x: self.x.div_euclid(CHUNK_SIZE as i32),
            z: self.z.div_euclid(CHUNK_SIZE as i32),
        }
    }

    /// Position within its chunk, always in `0..CHUNK_SIZE`.
    pub const fn offset_in_chunk(self) -> (u32, u32) {
        (
            self.x.rem_euclid(CHUNK_SIZE as i32) as u32,
            self.z.rem_euclid(CHUNK_SIZE as i32) as u32,
        )
    }

    /// The four edge-adjacent neighbours: -X, +X, -Z, +Z.
    pub const fn neighbors(self) -> [TilePos; 4] {
        [
            TilePos::new(self.x - 1, self.z),
            TilePos::new(self.x + 1, self.z),
            TilePos::new(self.x, self.z - 1),
            TilePos::new(self.x, self.z + 1),
        ]
    }
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// The tile at this chunk's minimum corner.
    pub const fn origin_tile(self) -> TilePos {
        TilePos::new(self.x * CHUNK_SIZE as i32, self.z * CHUNK_SIZE as i32)
    }
}

/// Which tile covers a point on the ground plane.
///
/// Floors, so the tile owns `[x, x+1)` — a point exactly on a boundary belongs
/// to the tile on its positive side. `floor` is exact in IEEE-754, so this is
/// not a source of platform divergence.
pub fn world_to_tile(world_x: f32, world_z: f32) -> TilePos {
    TilePos::new(world_x.floor() as i32, world_z.floor() as i32)
}

/// A finite, chunk-aligned grid of tiles.
///
/// Stored struct-of-arrays: heights and kinds in separate `Vec`s rather than a
/// `Vec<Tile>`. Meshing walks all the heights of a chunk and its neighbours,
/// then all the kinds, so keeping each contiguous is worth the small loss in
/// convenience.
///
/// Indexing is row-major with **X varying fastest**: `index = z * width + x`.
pub struct TileMap {
    width: u32,
    depth: u32,
    chunks_x: u32,
    chunks_z: u32,
    heights: Vec<i16>,
    kinds: Vec<TileKind>,
}

impl TileMap {
    /// Create a map sized in **chunks**, not tiles.
    ///
    /// Taking chunk counts makes a non-chunk-aligned world unrepresentable,
    /// which removes partial-chunk handling from the mesher and the culler
    /// entirely rather than making every one of them defend against it.
    ///
    /// Starts as flat ocean: every tile [`TileKind::Water`] at height 0.
    pub fn new(chunks_x: u32, chunks_z: u32) -> Self {
        assert!(
            chunks_x > 0 && chunks_z > 0,
            "a world needs at least one chunk"
        );
        let width = chunks_x * CHUNK_SIZE;
        let depth = chunks_z * CHUNK_SIZE;
        let count = (width as usize) * (depth as usize);
        Self {
            width,
            depth,
            chunks_x,
            chunks_z,
            heights: vec![0; count],
            kinds: vec![TileKind::Water; count],
        }
    }

    /// Width in tiles, along X.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Depth in tiles, along Z.
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    pub const fn chunks_x(&self) -> u32 {
        self.chunks_x
    }

    pub const fn chunks_z(&self) -> u32 {
        self.chunks_z
    }

    /// Total tiles. Useful for sizing and for the debug readout.
    pub const fn tile_count(&self) -> usize {
        (self.width as usize) * (self.depth as usize)
    }

    pub const fn chunk_count(&self) -> usize {
        (self.chunks_x as usize) * (self.chunks_z as usize)
    }

    /// Is this tile inside the world?
    pub const fn contains(&self, pos: TilePos) -> bool {
        pos.x >= 0 && pos.z >= 0 && (pos.x as u32) < self.width && (pos.z as u32) < self.depth
    }

    /// Flat index, or `None` outside the world.
    const fn index(&self, pos: TilePos) -> Option<usize> {
        if self.contains(pos) {
            Some((pos.z as usize) * (self.width as usize) + (pos.x as usize))
        } else {
            None
        }
    }

    /// Height in steps, or `None` outside the world.
    pub fn height(&self, pos: TilePos) -> Option<i16> {
        self.index(pos).map(|i| self.heights[i])
    }

    /// Height in steps, substituting `default` outside the world.
    ///
    /// The mesher needs this: deciding whether to emit a wall means comparing
    /// against a neighbour that may not exist, and at the world edge the
    /// desired answer is "treat the outside as far below" rather than "skip".
    pub fn height_or(&self, pos: TilePos, default: i16) -> i16 {
        self.height(pos).unwrap_or(default)
    }

    /// Kind, or `None` outside the world.
    pub fn kind(&self, pos: TilePos) -> Option<TileKind> {
        self.index(pos).map(|i| self.kinds[i])
    }

    /// Set the height. Silently ignores out-of-bounds writes so generation
    /// can work in terms of a region without clamping at every call site.
    pub fn set_height(&mut self, pos: TilePos, height: i16) {
        if let Some(i) = self.index(pos) {
            self.heights[i] = height;
        }
    }

    /// Set the kind. Out-of-bounds writes are ignored, as with `set_height`.
    pub fn set_kind(&mut self, pos: TilePos, kind: TileKind) {
        if let Some(i) = self.index(pos) {
            self.kinds[i] = kind;
        }
    }

    /// Raw height slice, for bulk generation and hashing.
    pub fn heights(&self) -> &[i16] {
        &self.heights
    }

    /// Raw kind slice, for bulk generation and hashing.
    pub fn kinds(&self) -> &[TileKind] {
        &self.kinds
    }

    /// A tile's origin: horizontal centre, at ground height.
    ///
    /// This is the point an object standing on the tile is positioned at.
    pub fn tile_origin(&self, pos: TilePos) -> Option<Vec3> {
        self.height(pos).map(|h| {
            Vec3::new(
                pos.x as f32 + 0.5,
                h as f32 * HEIGHT_STEP,
                pos.z as f32 + 0.5,
            )
        })
    }

    /// Is this chunk inside the world?
    pub const fn contains_chunk(&self, chunk: ChunkPos) -> bool {
        chunk.x >= 0
            && chunk.z >= 0
            && (chunk.x as u32) < self.chunks_x
            && (chunk.z as u32) < self.chunks_z
    }

    /// Every chunk, in row-major order with X varying fastest.
    pub fn chunks(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        let (cx, cz) = (self.chunks_x, self.chunks_z);
        (0..cz).flat_map(move |z| (0..cx).map(move |x| ChunkPos::new(x as i32, z as i32)))
    }

    /// Every tile in a chunk, in row-major order.
    ///
    /// Returns an empty iterator for a chunk outside the world, so callers
    /// iterating a stale chunk list produce nothing rather than panicking.
    pub fn chunk_tiles(&self, chunk: ChunkPos) -> impl Iterator<Item = TilePos> + '_ {
        let inside = self.contains_chunk(chunk);
        let origin = chunk.origin_tile();
        let n = if inside { CHUNK_SIZE } else { 0 };
        (0..n).flat_map(move |dz| {
            (0..n).map(move |dx| TilePos::new(origin.x + dx as i32, origin.z + dz as i32))
        })
    }

    /// World-space axis-aligned bounds of a chunk, for frustum culling.
    ///
    /// Y comes from the actual minimum and maximum height in the chunk, so the
    /// box is tight rather than spanning the world's whole vertical range. The
    /// minimum is extended down by one step to cover the wall faces that hang
    /// below the lowest top face.
    ///
    /// `None` for a chunk outside the world.
    pub fn chunk_bounds(&self, chunk: ChunkPos) -> Option<(Vec3, Vec3)> {
        if !self.contains_chunk(chunk) {
            return None;
        }
        let origin = chunk.origin_tile();

        let mut min_h = i16::MAX;
        let mut max_h = i16::MIN;
        for dz in 0..CHUNK_SIZE as i32 {
            let row = ((origin.z + dz) as usize) * (self.width as usize) + (origin.x as usize);
            for h in &self.heights[row..row + CHUNK_SIZE as usize] {
                min_h = min_h.min(*h);
                max_h = max_h.max(*h);
            }
        }

        let min = Vec3::new(
            origin.x as f32,
            (min_h as f32 - 1.0) * HEIGHT_STEP,
            origin.z as f32,
        );
        let max = Vec3::new(
            (origin.x + CHUNK_SIZE as i32) as f32,
            max_h as f32 * HEIGHT_STEP,
            (origin.z + CHUNK_SIZE as i32) as f32,
        );
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TileMap {
        TileMap::new(2, 3) // 64 × 96 tiles
    }

    #[test]
    fn dimensions_derive_from_chunk_counts() {
        let m = map();
        assert_eq!(m.width(), 2 * CHUNK_SIZE);
        assert_eq!(m.depth(), 3 * CHUNK_SIZE);
        assert_eq!(
            m.tile_count(),
            (2 * CHUNK_SIZE as usize) * (3 * CHUNK_SIZE as usize)
        );
        assert_eq!(m.chunk_count(), 6);
    }

    #[test]
    fn new_world_is_flat_ocean() {
        let m = map();
        assert!(m.heights().iter().all(|h| *h == 0));
        assert!(m.kinds().iter().all(|k| *k == TileKind::Water));
    }

    #[test]
    fn contains_rejects_every_edge_and_beyond() {
        let m = map();
        let (w, d) = (m.width() as i32, m.depth() as i32);
        assert!(m.contains(TilePos::new(0, 0)));
        assert!(m.contains(TilePos::new(w - 1, d - 1)));
        // One past each edge, and negatives.
        assert!(!m.contains(TilePos::new(w, 0)));
        assert!(!m.contains(TilePos::new(0, d)));
        assert!(!m.contains(TilePos::new(-1, 0)));
        assert!(!m.contains(TilePos::new(0, -1)));
    }

    #[test]
    fn index_is_row_major_with_x_fastest() {
        let mut m = map();
        // Two tiles adjacent in X must be adjacent in memory; two adjacent in
        // Z must be `width` apart. Verified through the public API by writing
        // distinct heights and reading the raw slice.
        m.set_height(TilePos::new(0, 0), 10);
        m.set_height(TilePos::new(1, 0), 20);
        m.set_height(TilePos::new(0, 1), 30);
        let h = m.heights();
        assert_eq!(h[0], 10);
        assert_eq!(h[1], 20);
        assert_eq!(h[m.width() as usize], 30);
    }

    #[test]
    fn height_and_kind_are_none_outside_the_world() {
        let m = map();
        assert_eq!(m.height(TilePos::new(-1, 0)), None);
        assert_eq!(m.kind(TilePos::new(0, -1)), None);
        assert_eq!(m.height(TilePos::new(m.width() as i32, 0)), None);
    }

    #[test]
    fn out_of_bounds_writes_are_ignored_not_panics() {
        let mut m = map();
        m.set_height(TilePos::new(-5, -5), 99);
        m.set_kind(TilePos::new(9999, 9999), TileKind::Snow);
        // Nothing was written anywhere.
        assert!(m.heights().iter().all(|h| *h == 0));
        assert!(m.kinds().iter().all(|k| *k == TileKind::Water));
    }

    #[test]
    fn height_or_substitutes_only_outside() {
        let mut m = map();
        m.set_height(TilePos::new(3, 3), 7);
        assert_eq!(m.height_or(TilePos::new(3, 3), -99), 7);
        assert_eq!(m.height_or(TilePos::new(-1, 3), -99), -99);
    }

    #[test]
    fn world_to_tile_floors_including_negatives() {
        assert_eq!(world_to_tile(0.0, 0.0), TilePos::new(0, 0));
        assert_eq!(world_to_tile(0.9, 0.9), TilePos::new(0, 0));
        assert_eq!(world_to_tile(1.0, 1.0), TilePos::new(1, 1));
        // Truncation would give 0 here; flooring gives -1. A point just left
        // of the origin is in tile -1, not tile 0.
        assert_eq!(world_to_tile(-0.1, -0.1), TilePos::new(-1, -1));
        assert_eq!(world_to_tile(-1.0, -1.0), TilePos::new(-1, -1));
    }

    #[test]
    fn tile_origin_is_the_centre_at_ground_height() {
        let mut m = map();
        m.set_height(TilePos::new(4, 6), 3);
        let o = m.tile_origin(TilePos::new(4, 6)).unwrap();
        assert_eq!(o.x, 4.5);
        assert_eq!(o.z, 6.5);
        assert_eq!(o.y, 3.0 * HEIGHT_STEP);
        assert_eq!(m.tile_origin(TilePos::new(-1, 0)), None);
    }

    #[test]
    fn tile_to_chunk_floors_rather_than_truncating() {
        let n = CHUNK_SIZE as i32;
        assert_eq!(TilePos::new(0, 0).chunk(), ChunkPos::new(0, 0));
        assert_eq!(TilePos::new(n - 1, n - 1).chunk(), ChunkPos::new(0, 0));
        assert_eq!(TilePos::new(n, n).chunk(), ChunkPos::new(1, 1));
        // The bug this guards: `-1 / 32` truncates to 0, putting tile -1 in
        // the same chunk as tile 0. div_euclid gives -1.
        assert_eq!(TilePos::new(-1, -1).chunk(), ChunkPos::new(-1, -1));
        assert_eq!(TilePos::new(-n, -n).chunk(), ChunkPos::new(-1, -1));
        assert_eq!(TilePos::new(-n - 1, 0).chunk(), ChunkPos::new(-2, 0));
    }

    #[test]
    fn offset_in_chunk_is_always_within_the_chunk() {
        let n = CHUNK_SIZE as i32;
        assert_eq!(TilePos::new(0, 0).offset_in_chunk(), (0, 0));
        assert_eq!(TilePos::new(n - 1, n - 1).offset_in_chunk(), (31, 31));
        assert_eq!(TilePos::new(n, n).offset_in_chunk(), (0, 0));
        // Negative tiles still land inside their (negative) chunk.
        assert_eq!(TilePos::new(-1, -1).offset_in_chunk(), (31, 31));
    }

    #[test]
    fn chunk_origin_and_tile_position_round_trip() {
        let m = map();
        for chunk in m.chunks() {
            let origin = chunk.origin_tile();
            assert_eq!(
                origin.chunk(),
                chunk,
                "chunk origin must map back to its chunk"
            );
            assert_eq!(origin.offset_in_chunk(), (0, 0));
        }
    }

    #[test]
    fn chunks_enumerates_every_chunk_once() {
        let m = map();
        let all: Vec<_> = m.chunks().collect();
        assert_eq!(all.len(), m.chunk_count());
        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "chunks() yielded a duplicate");
        assert!(all.iter().all(|c| m.contains_chunk(*c)));
    }

    #[test]
    fn chunk_tiles_covers_the_chunk_exactly() {
        let m = map();
        let chunk = ChunkPos::new(1, 2);
        let tiles: Vec<_> = m.chunk_tiles(chunk).collect();
        assert_eq!(tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
        // Every tile belongs to this chunk, and all are inside the world.
        assert!(tiles.iter().all(|t| t.chunk() == chunk));
        assert!(tiles.iter().all(|t| m.contains(*t)));
        let mut sorted = tiles.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), tiles.len(), "chunk_tiles yielded a duplicate");
    }

    #[test]
    fn chunk_tiles_is_empty_outside_the_world() {
        let m = map();
        assert_eq!(m.chunk_tiles(ChunkPos::new(-1, 0)).count(), 0);
        assert_eq!(m.chunk_tiles(ChunkPos::new(99, 99)).count(), 0);
    }

    #[test]
    fn every_tile_belongs_to_exactly_one_chunk() {
        let m = map();
        let mut seen = vec![false; m.tile_count()];
        for chunk in m.chunks() {
            for t in m.chunk_tiles(chunk) {
                let i = (t.z as usize) * (m.width() as usize) + (t.x as usize);
                assert!(!seen[i], "tile {t:?} covered by more than one chunk");
                seen[i] = true;
            }
        }
        assert!(seen.iter().all(|s| *s), "some tile is in no chunk");
    }

    #[test]
    fn neighbors_are_the_four_edge_adjacent_tiles() {
        let n = TilePos::new(5, 7).neighbors();
        assert_eq!(n[0], TilePos::new(4, 7));
        assert_eq!(n[1], TilePos::new(6, 7));
        assert_eq!(n[2], TilePos::new(5, 6));
        assert_eq!(n[3], TilePos::new(5, 8));
    }

    #[test]
    fn corner_tile_has_two_neighbors_inside_the_world() {
        let m = map();
        let inside = TilePos::new(0, 0)
            .neighbors()
            .iter()
            .filter(|p| m.contains(**p))
            .count();
        assert_eq!(
            inside, 2,
            "the origin corner should have exactly 2 in-world neighbours"
        );

        let far = TilePos::new(m.width() as i32 - 1, m.depth() as i32 - 1);
        let inside = far.neighbors().iter().filter(|p| m.contains(**p)).count();
        assert_eq!(
            inside, 2,
            "the far corner should have exactly 2 in-world neighbours"
        );
    }

    #[test]
    fn edge_tile_has_three_neighbors_inside_the_world() {
        let m = map();
        let inside = TilePos::new(5, 0)
            .neighbors()
            .iter()
            .filter(|p| m.contains(**p))
            .count();
        assert_eq!(inside, 3);
    }

    #[test]
    fn chunk_bounds_are_tight_in_y_and_span_the_chunk_in_xz() {
        let mut m = map();
        let chunk = ChunkPos::new(1, 1);
        let origin = chunk.origin_tile();
        m.set_height(origin, -4);
        m.set_height(TilePos::new(origin.x + 5, origin.z + 5), 9);

        let (min, max) = m.chunk_bounds(chunk).unwrap();
        assert_eq!(min.x, origin.x as f32);
        assert_eq!(min.z, origin.z as f32);
        assert_eq!(max.x, (origin.x + CHUNK_SIZE as i32) as f32);
        assert_eq!(max.z, (origin.z + CHUNK_SIZE as i32) as f32);
        // Extended one step below the lowest tile to cover hanging walls.
        assert_eq!(min.y, (-4.0 - 1.0) * HEIGHT_STEP);
        assert_eq!(max.y, 9.0 * HEIGHT_STEP);
    }

    #[test]
    fn chunk_bounds_is_none_outside_the_world() {
        let m = map();
        assert!(m.chunk_bounds(ChunkPos::new(-1, 0)).is_none());
        assert!(m.chunk_bounds(ChunkPos::new(0, 99)).is_none());
    }
}
