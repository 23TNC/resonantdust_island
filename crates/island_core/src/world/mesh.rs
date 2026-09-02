//! Turning tiles into triangles.
//!
//! One mesh per chunk, built on the CPU and uploaded once. Terrain is stepped,
//! so every tile is a flat quad at its own height and every height change
//! between neighbours becomes a vertical wall.
//!
//! # Walls are not optional
//!
//! A heightmap that emits only top faces renders as a set of floating plateaus
//! with the sky visible through every height change. The walls are what make
//! it read as terrain. See `issues.md` §1.
//!
//! # No vertex sharing
//!
//! Adjacent tiles do not share vertices even when they meet exactly. Flat
//! shading needs one normal per face, and a shared vertex can only carry one
//! normal — sharing would round every step into a slope. The cost is four
//! vertices per quad instead of an amortised one.

use super::{ChunkPos, TileKind, TileMap, TilePos, CHUNK_SIZE, HEIGHT_STEP};

/// How far below an edge tile the world-boundary skirt hangs, in height steps.
///
/// Without a skirt the world's outer rim is an open edge you can see under,
/// which reads as a rendering fault rather than as the end of the map. This
/// gives it a plinth to sit on.
const SKIRT_STEPS: i16 = 8;

/// A single terrain vertex.
///
/// `#[repr(C)]` and `Pod` so it can be uploaded straight to the GPU with no
/// per-vertex conversion. Nine `f32`s with no padding, 36 bytes.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

/// CPU-side geometry for one chunk.
#[derive(Debug, Default, Clone)]
pub struct ChunkMesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of quads. Every face emitted here is a quad, so this is the
    /// natural unit for reasoning about what the mesher produced.
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 4
    }

    /// Append a quad whose corners are already in counter-clockwise order as
    /// seen from `normal`.
    ///
    /// Winding matters: wgpu treats counter-clockwise as front-facing and the
    /// terrain pipeline culls back faces, so a quad wound the wrong way is
    /// simply invisible from the side it should be seen from.
    fn push_quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 3]) {
        let base = self.vertices.len() as u32;
        for position in corners {
            self.vertices.push(Vertex {
                position,
                normal,
                color,
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// World Y for a height in steps.
fn world_y(height: i16) -> f32 {
    height as f32 * HEIGHT_STEP
}

/// The four horizontal directions a wall can face, as
/// `(neighbour dx, neighbour dz, outward normal)`.
const WALL_DIRECTIONS: [(i32, i32, [f32; 3]); 4] = [
    (-1, 0, [-1.0, 0.0, 0.0]),
    (1, 0, [1.0, 0.0, 0.0]),
    (0, -1, [0.0, 0.0, -1.0]),
    (0, 1, [0.0, 0.0, 1.0]),
];

/// Corners of a wall on one side of tile `(x, z)`, counter-clockwise as seen
/// from outside.
///
/// Each ordering was derived from the cross product of its own first triangle
/// and is checked by `every_triangle_winding_matches_its_normal`, which
/// recomputes the geometric normal of every emitted triangle. Deriving these
/// by eye is exactly the sort of thing that is wrong once and never noticed.
fn wall_corners(x: i32, z: i32, y_low: f32, y_high: f32, dx: i32, dz: i32) -> [[f32; 3]; 4] {
    let (x0, x1) = (x as f32, (x + 1) as f32);
    let (z0, z1) = (z as f32, (z + 1) as f32);

    match (dx, dz) {
        (-1, 0) => [
            [x0, y_low, z0],
            [x0, y_low, z1],
            [x0, y_high, z1],
            [x0, y_high, z0],
        ],
        (1, 0) => [
            [x1, y_low, z1],
            [x1, y_low, z0],
            [x1, y_high, z0],
            [x1, y_high, z1],
        ],
        (0, -1) => [
            [x1, y_low, z0],
            [x0, y_low, z0],
            [x0, y_high, z0],
            [x1, y_high, z0],
        ],
        (0, 1) => [
            [x0, y_low, z1],
            [x1, y_low, z1],
            [x1, y_high, z1],
            [x0, y_high, z1],
        ],
        _ => unreachable!("wall directions are the four axis-aligned neighbours"),
    }
}

/// Build the mesh for one chunk.
///
/// Reads neighbouring tiles across chunk borders, so a chunk's mesh depends on
/// its neighbours' heights. Rebuilding after an edit therefore means
/// rebuilding the edited chunk *and* any neighbour it shares a border with.
///
/// Returns an empty mesh for a chunk outside the world.
pub fn mesh_chunk(map: &TileMap, chunk: ChunkPos) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    if !map.contains_chunk(chunk) {
        return mesh;
    }

    let origin = chunk.origin_tile();

    for dz in 0..CHUNK_SIZE as i32 {
        for dx in 0..CHUNK_SIZE as i32 {
            let pos = TilePos::new(origin.x + dx, origin.z + dz);
            let Some(height) = map.height(pos) else {
                continue;
            };
            let kind = map.kind(pos).unwrap_or(TileKind::Water);
            let color = kind.color();
            let y = world_y(height);
            let (x0, x1) = (pos.x as f32, (pos.x + 1) as f32);
            let (z0, z1) = (pos.z as f32, (pos.z + 1) as f32);

            // Top face, counter-clockwise seen from +Y.
            mesh.push_quad(
                [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]],
                [0.0, 1.0, 0.0],
                color,
            );

            for (ndx, ndz, normal) in WALL_DIRECTIONS {
                let neighbor = TilePos::new(pos.x + ndx, pos.z + ndz);

                // Outside the world, hang a skirt rather than leaving an open
                // edge. Inside, a wall is only needed where the neighbour is
                // genuinely lower — between equal tiles it would be geometry
                // sealed inside the terrain, invisible and paid for anyway.
                let neighbor_height = map.height_or(neighbor, height.saturating_sub(SKIRT_STEPS));
                if neighbor_height >= height {
                    continue;
                }

                mesh.push_quad(
                    wall_corners(pos.x, pos.z, world_y(neighbor_height), y, ndx, ndz),
                    normal,
                    color,
                );
            }
        }
    }

    mesh
}

/// Build meshes for every chunk in the world.
pub fn mesh_world(map: &TileMap) -> Vec<(ChunkPos, ChunkMesh)> {
    map.chunks()
        .map(|chunk| (chunk, mesh_chunk(map, chunk)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldParams;

    const TILES_PER_CHUNK: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

    /// A 3×3-chunk world so chunk (1,1) is fully interior — its neighbours all
    /// exist, so it emits no skirt and the counts are exactly predictable.
    fn interior_world() -> TileMap {
        TileMap::new(3, 3)
    }

    fn interior_chunk() -> ChunkPos {
        ChunkPos::new(1, 1)
    }

    /// Geometric normal of a triangle, from the cross product of two edges.
    fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    // ---- Winding: the check that the hand-derived corner orders are right ----

    /// Every triangle's geometric normal must point the same way as the normal
    /// stored on its vertices.
    ///
    /// Back faces are culled, so a quad wound the wrong way is not subtly
    /// wrong — it is *invisible*, and invisible from precisely the side you
    /// were trying to look at. Recomputing the winding from the emitted
    /// geometry is the only way to know the four hand-derived corner orders in
    /// `wall_corners` are correct.
    #[test]
    fn every_triangle_winding_matches_its_normal() {
        let map = WorldParams {
            chunks_x: 3,
            chunks_z: 3,
            ..WorldParams::default()
        }
        .generate();

        let mut checked = 0;
        for (_, mesh) in mesh_world(&map) {
            for tri in mesh.indices().chunks_exact(3) {
                let v = [
                    mesh.vertices()[tri[0] as usize],
                    mesh.vertices()[tri[1] as usize],
                    mesh.vertices()[tri[2] as usize],
                ];
                // All three vertices of a face carry the same normal.
                assert_eq!(v[0].normal, v[1].normal);
                assert_eq!(v[0].normal, v[2].normal);

                let geometric = face_normal(v[0].position, v[1].position, v[2].position);
                assert!(
                    dot(geometric, v[0].normal) > 0.0,
                    "triangle at {:?} is wound backwards: geometric normal {:?} vs declared {:?}",
                    v[0].position,
                    geometric,
                    v[0].normal
                );
                checked += 1;
            }
        }
        assert!(checked > 10_000, "only checked {checked} triangles");
    }

    #[test]
    fn top_faces_point_up() {
        let map = interior_world();
        let mesh = mesh_chunk(&map, interior_chunk());
        let up = mesh
            .vertices()
            .iter()
            .filter(|v| v.normal == [0.0, 1.0, 0.0])
            .count();
        assert_eq!(up, TILES_PER_CHUNK * 4, "one upward quad per tile");
    }

    // ---- Exact counts ----

    /// A flat interior chunk has no height changes anywhere, including across
    /// its borders, so it is exactly one quad per tile and nothing else.
    #[test]
    fn flat_interior_chunk_is_one_quad_per_tile_with_no_walls() {
        let map = interior_world(); // fresh worlds are flat ocean at height 0
        let mesh = mesh_chunk(&map, interior_chunk());

        assert_eq!(mesh.quad_count(), TILES_PER_CHUNK);
        assert_eq!(mesh.vertices().len(), TILES_PER_CHUNK * 4);
        assert_eq!(mesh.indices().len(), TILES_PER_CHUNK * 6);
        assert_eq!(mesh.triangle_count(), TILES_PER_CHUNK * 2);
        assert!(
            mesh.vertices().iter().all(|v| v.normal == [0.0, 1.0, 0.0]),
            "a flat chunk should emit only top faces"
        );
    }

    /// One raised tile, surrounded by flat ground, exposes exactly four walls.
    #[test]
    fn a_single_raised_tile_adds_four_walls() {
        let mut map = interior_world();
        let flat = mesh_chunk(&map, interior_chunk()).quad_count();

        let middle = TilePos::new(
            interior_chunk().origin_tile().x + 10,
            interior_chunk().origin_tile().z + 10,
        );
        map.set_height(middle, 3);

        let mesh = mesh_chunk(&map, interior_chunk());
        assert_eq!(
            mesh.quad_count(),
            flat + 4,
            "a raised tile should expose one wall per side"
        );
    }

    /// A lowered tile puts the walls on its *neighbours*, not on itself: each
    /// of the four surrounding tiles is now higher than one of its own
    /// neighbours. The total is the same four walls.
    #[test]
    fn a_single_lowered_tile_also_adds_four_walls() {
        let mut map = interior_world();
        let flat = mesh_chunk(&map, interior_chunk()).quad_count();

        let middle = TilePos::new(
            interior_chunk().origin_tile().x + 10,
            interior_chunk().origin_tile().z + 10,
        );
        map.set_height(middle, -3);

        let mesh = mesh_chunk(&map, interior_chunk());
        assert_eq!(mesh.quad_count(), flat + 4);
    }

    /// Walls span the full height difference, not a fixed step.
    #[test]
    fn wall_height_matches_the_height_difference() {
        let mut map = interior_world();
        let origin = interior_chunk().origin_tile();
        let raised = TilePos::new(origin.x + 5, origin.z + 5);
        map.set_height(raised, 7);

        let mesh = mesh_chunk(&map, interior_chunk());
        let wall_ys: Vec<f32> = mesh
            .vertices()
            .iter()
            .filter(|v| v.normal[1] == 0.0)
            .map(|v| v.position[1])
            .collect();

        assert!(!wall_ys.is_empty(), "expected walls");
        let lo = wall_ys.iter().copied().fold(f32::MAX, f32::min);
        let hi = wall_ys.iter().copied().fold(f32::MIN, f32::max);
        assert_eq!(lo, world_y(0), "walls should reach down to the neighbour");
        assert_eq!(hi, world_y(7), "walls should reach up to the tile top");
    }

    // ---- World edge skirt ----

    /// The world's outer rim has no neighbour to compare against, so it hangs
    /// a skirt. Without it you can see under the edge of the map.
    #[test]
    fn edge_chunk_emits_a_skirt_along_the_world_boundary() {
        let map = TileMap::new(3, 3);

        let interior = mesh_chunk(&map, ChunkPos::new(1, 1)).quad_count();
        let corner = mesh_chunk(&map, ChunkPos::new(0, 0)).quad_count();
        let edge = mesh_chunk(&map, ChunkPos::new(1, 0)).quad_count();

        // An edge chunk borders the world on one side: one skirt quad per tile
        // along that side.
        assert_eq!(edge, interior + CHUNK_SIZE as usize);
        // A corner chunk borders on two sides.
        assert_eq!(corner, interior + 2 * CHUNK_SIZE as usize);
    }

    #[test]
    fn the_skirt_hangs_below_the_edge_tiles() {
        let map = TileMap::new(2, 2);
        let mesh = mesh_chunk(&map, ChunkPos::new(0, 0));
        let lowest = mesh
            .vertices()
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MAX, f32::min);
        assert_eq!(lowest, world_y(-SKIRT_STEPS));
    }

    // ---- Structural soundness ----

    #[test]
    fn every_index_is_in_range_and_triangles_are_whole() {
        let map = WorldParams {
            chunks_x: 2,
            chunks_z: 2,
            ..WorldParams::default()
        }
        .generate();

        for (chunk, mesh) in mesh_world(&map) {
            assert_eq!(
                mesh.indices().len() % 3,
                0,
                "{chunk:?} has a partial triangle"
            );
            let n = mesh.vertices().len() as u32;
            assert!(
                mesh.indices().iter().all(|i| *i < n),
                "{chunk:?} has an index past the end of its vertex buffer"
            );
        }
    }

    #[test]
    fn vertices_carry_their_tile_colour() {
        let mut map = interior_world();
        let origin = interior_chunk().origin_tile();
        map.set_kind(TilePos::new(origin.x, origin.z), TileKind::Snow);

        let mesh = mesh_chunk(&map, interior_chunk());
        assert_eq!(mesh.vertices()[0].color, TileKind::Snow.color());
    }

    #[test]
    fn a_chunk_outside_the_world_meshes_to_nothing() {
        let map = interior_world();
        assert!(mesh_chunk(&map, ChunkPos::new(-1, 0)).is_empty());
        assert!(mesh_chunk(&map, ChunkPos::new(99, 99)).is_empty());
    }

    #[test]
    fn mesh_world_covers_every_chunk() {
        let map = TileMap::new(2, 3);
        let meshes = mesh_world(&map);
        assert_eq!(meshes.len(), map.chunk_count());
        assert!(meshes.iter().all(|(_, m)| !m.is_empty()));
    }

    /// Chunk meshes read across their borders, so an edit near a border
    /// changes the neighbouring chunk's mesh too. Anything doing incremental
    /// rebuilds has to know this.
    #[test]
    fn editing_a_border_tile_changes_the_neighbouring_chunk() {
        let mut map = interior_world();
        let before = mesh_chunk(&map, ChunkPos::new(1, 1)).quad_count();

        // Last tile of chunk (0,1): touches chunk (1,1) across the border.
        let border = TilePos::new(CHUNK_SIZE as i32 - 1, CHUNK_SIZE as i32 + 5);
        map.set_height(border, -5);

        let after = mesh_chunk(&map, ChunkPos::new(1, 1)).quad_count();
        assert_eq!(after, before + 1, "the neighbour should have gained a wall");
    }

    /// A sanity bound on real terrain: five faces per tile is the absolute
    /// worst case, and real terrain is nowhere near it.
    #[test]
    fn generated_terrain_emits_a_reasonable_amount_of_geometry() {
        let map = WorldParams::default().generate();
        let meshes = mesh_world(&map);
        let quads: usize = meshes.iter().map(|(_, m)| m.quad_count()).sum();

        let tiles = map.tile_count();
        assert!(quads > tiles, "terrain with relief must emit some walls");
        assert!(
            quads < tiles * 5,
            "{quads} quads for {tiles} tiles exceeds the theoretical maximum"
        );
    }
}

#[cfg(test)]
mod measure {
    use super::*;
    use crate::world::WorldParams;
    use std::time::Instant;

    /// Real geometry and timing numbers for the default world, so group F's
    /// budget is measured rather than guessed. See `issues.md` §6.
    /// `cargo test -p island_core --release -- --ignored --nocapture measure_mesh`
    #[test]
    #[ignore = "diagnostic, not an assertion"]
    fn measure_mesh() {
        let params = WorldParams::default();

        let t0 = Instant::now();
        let map = params.generate();
        let generate_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let t1 = Instant::now();
        let meshes = mesh_world(&map);
        let mesh_ms = t1.elapsed().as_secs_f64() * 1000.0;

        let quads: usize = meshes.iter().map(|(_, m)| m.quad_count()).sum();
        let verts: usize = meshes.iter().map(|(_, m)| m.vertices().len()).sum();
        let indices: usize = meshes.iter().map(|(_, m)| m.indices().len()).sum();
        let tiles = map.tile_count();

        let vertex_bytes = verts * std::mem::size_of::<Vertex>();
        let index_bytes = indices * std::mem::size_of::<u32>();
        let mb = |b: usize| b as f64 / (1024.0 * 1024.0);

        println!(
            "world        {}x{} = {tiles} tiles, {} chunks",
            map.width(),
            map.depth(),
            meshes.len()
        );
        println!("generate     {generate_ms:.1} ms");
        println!("mesh         {mesh_ms:.1} ms");
        println!(
            "quads        {quads}  ({:.2} per tile)",
            quads as f64 / tiles as f64
        );
        println!("triangles    {}", indices / 3);
        println!("vertices     {verts}");
        println!(
            "vertex data  {:.2} MiB  ({} B/vertex)",
            mb(vertex_bytes),
            std::mem::size_of::<Vertex>()
        );
        println!("index data   {:.2} MiB  (u32)", mb(index_bytes));
        println!("total        {:.2} MiB", mb(vertex_bytes + index_bytes));
        println!(
            "  as u16     {:.2} MiB  (indices halved)",
            mb(vertex_bytes + index_bytes / 2)
        );

        let largest = meshes
            .iter()
            .map(|(_, m)| m.vertices().len())
            .max()
            .unwrap();
        println!(
            "largest chunk {largest} vertices  (u16 index limit is {})",
            u16::MAX
        );

        // The cost that actually recurs: re-meshing after a terrain edit.
        // mesh_world is a startup cost paid once; this is the one a digging or
        // building system pays, and it is per-chunk rather than per-world.
        const REPS: u32 = 200;
        let chunk = ChunkPos::new(4, 4);
        let t2 = Instant::now();
        for _ in 0..REPS {
            std::hint::black_box(mesh_chunk(&map, std::hint::black_box(chunk)));
        }
        let per_chunk_us = t2.elapsed().as_secs_f64() * 1e6 / f64::from(REPS);
        println!();
        println!("re-mesh 1 chunk    {per_chunk_us:.0} us");
        println!("  edit mid-chunk   {per_chunk_us:.0} us   (1 chunk)");
        println!(
            "  edit on a border {:.0} us   (2 chunks)",
            per_chunk_us * 2.0
        );
        println!(
            "  edit on a corner {:.0} us   (4 chunks)",
            per_chunk_us * 4.0
        );
        println!("  frame budget     16667 us  (60 fps)");
    }
}
