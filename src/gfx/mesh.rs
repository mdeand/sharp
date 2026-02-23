use std::collections::HashMap;

use crate::gfx::vertex::Vertex;

/// Generate a unit-radius icosphere by subdividing an icosahedron.
/// `subdivisions` controls detail level (0 = 20 tris, 1 = 80, 2 = 320, 3 = 1280).
/// 2 is a good default for an aim-trainer target.
pub fn create_sphere(subdivisions: u32) -> (Vec<Vertex>, Vec<u32>) {
  let t = (1.0 + 5.0f32.sqrt()) / 2.0;

  let mut positions: Vec<[f32; 3]> = vec![
    [-1.0, t, 0.0],
    [1.0, t, 0.0],
    [-1.0, -t, 0.0],
    [1.0, -t, 0.0],
    [0.0, -1.0, t],
    [0.0, 1.0, t],
    [0.0, -1.0, -t],
    [0.0, 1.0, -t],
    [t, 0.0, -1.0],
    [t, 0.0, 1.0],
    [-t, 0.0, -1.0],
    [-t, 0.0, 1.0],
  ];

  // Normalize initial vertices onto the unit sphere
  for p in positions.iter_mut() {
    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    p[0] /= len;
    p[1] /= len;
    p[2] /= len;
  }

  let mut indices: Vec<u32> = vec![
    0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1,
    8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
  ];

  // Midpoint cache: maps an edge (smaller_index, larger_index) -> midpoint vertex index.
  // This ensures each shared edge is only split once.
  let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();

  let get_midpoint =
    |a: u32, b: u32, positions: &mut Vec<[f32; 3]>, cache: &mut HashMap<(u32, u32), u32>| -> u32 {
      let key = if a < b { (a, b) } else { (b, a) };
      if let Some(&idx) = cache.get(&key) {
        return idx;
      }
      let pa = positions[a as usize];
      let pb = positions[b as usize];
      let mut mid = [
        (pa[0] + pb[0]) * 0.5,
        (pa[1] + pb[1]) * 0.5,
        (pa[2] + pb[2]) * 0.5,
      ];
      // Project onto unit sphere
      let len = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2]).sqrt();
      mid[0] /= len;
      mid[1] /= len;
      mid[2] /= len;

      let idx = positions.len() as u32;
      positions.push(mid);
      cache.insert(key, idx);
      idx
    };

  for _ in 0..subdivisions {
    let mut new_indices = Vec::with_capacity(indices.len() * 4);

    for tri in indices.chunks(3) {
      let (a, b, c) = (tri[0], tri[1], tri[2]);

      let ab = get_midpoint(a, b, &mut positions, &mut midpoint_cache);
      let bc = get_midpoint(b, c, &mut positions, &mut midpoint_cache);
      let ca = get_midpoint(c, a, &mut positions, &mut midpoint_cache);

      new_indices.extend_from_slice(&[a, ab, ca, b, bc, ab, c, ca, bc, ab, bc, ca]);
    }

    indices = new_indices;
    midpoint_cache.clear();
  }

  let vertices: Vec<Vertex> = positions
    .iter()
    .map(|p| Vertex {
      position: *p,
      color: [0.8, 0.1, 0.1],
    })
    .collect();

  (vertices, indices)
}
