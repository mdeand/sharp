use cgmath::InnerSpace;
use mlua::prelude::*;
use std::path::Path;

pub struct TargetComponents {
  pub positions: Vec<cgmath::Point3<f32>>,
  pub velocities: Vec<cgmath::Vector3<f32>>,
  pub radii: Vec<f32>,
  /// Total height of each target (for capsule/pill shapes).
  /// When height <= 2*radius the target is rendered as a sphere.
  pub heights: Vec<f32>,
  pub active: Vec<bool>,
  pub spawn_times: Vec<f32>,
}

/// Camera frustum info needed for spawning targets in view.
pub struct FrustumParams {
  pub eye: cgmath::Point3<f32>,
  pub forward: cgmath::Vector3<f32>,
  pub fovy_deg: f32,
  pub aspect: f32,
}

/// Tracking accuracy statistics.
pub struct TrackingStats {
  /// Total frames where the player was holding fire.
  pub frames_tracking: u64,
  /// Frames where the crosshair was on the target while firing.
  pub frames_on_target: u64,
}

impl TrackingStats {
  pub fn accuracy_pct(&self) -> f32 {
    if self.frames_tracking == 0 {
      0.0
    } else {
      (self.frames_on_target as f32 / self.frames_tracking as f32) * 100.0
    }
  }
}

pub struct Scenario {
  pub targets: TargetComponents,
  pub timer: f32,
  pub score: u32,
  pub stats: TrackingStats,
  /// Is the player currently holding fire (LMB)?
  pub firing: bool,
  /// Human-readable name from the Lua script.
  pub name: String,
  /// The Lua VM that owns the scenario logic.
  lua: Lua,
}

impl Scenario {
  /// Load a scenario from a Lua script.
  ///
  /// The script must return a table with:
  ///   - `name`  (string)       – display name
  ///   - `init(ctx)`            – returns `{ {x,y,z,radius}, … }`
  ///   - `update(targets, ctx)` – returns updated targets table
  pub fn load(script_path: &Path, frustum: &FrustumParams) -> Self {
    let lua = Lua::new();

    let script = std::fs::read_to_string(script_path)
      .unwrap_or_else(|e| panic!("Failed to read scenario script {:?}: {}", script_path, e));

    let scenario_table: LuaTable = lua
      .load(&script)
      .set_name(script_path.to_string_lossy())
      .eval()
      .unwrap_or_else(|e| panic!("Failed to load scenario script: {}", e));

    let name: String = scenario_table
      .get("name")
      .unwrap_or_else(|_| "Unnamed".to_string());

    // Keep the table accessible for update() calls.
    lua
      .globals()
      .set("_scenario", scenario_table.clone())
      .unwrap();

    // --- call init(ctx) -----------------------------------------------------
    let ctx = lua.create_table().unwrap();
    let fwd = frustum.forward.normalize();
    ctx.set("eye_x", frustum.eye.x).unwrap();
    ctx.set("eye_y", frustum.eye.y).unwrap();
    ctx.set("eye_z", frustum.eye.z).unwrap();
    ctx.set("forward_x", fwd.x).unwrap();
    ctx.set("forward_y", fwd.y).unwrap();
    ctx.set("forward_z", fwd.z).unwrap();
    ctx.set("fovy", frustum.fovy_deg).unwrap();
    ctx.set("aspect", frustum.aspect).unwrap();

    let init_fn: LuaFunction = scenario_table
      .get("init")
      .expect("Scenario script must define an 'init' function");

    let result: LuaTable = init_fn
      .call(ctx)
      .expect("Scenario init() must return a table of targets");

    // --- parse targets ------------------------------------------------------
    let mut positions = Vec::new();
    let mut radii = Vec::new();
    let mut heights = Vec::new();
    let mut active = Vec::new();
    let mut spawn_times = Vec::new();
    let mut velocities = Vec::new();

    for pair in result.pairs::<i64, LuaTable>() {
      let (_, target) = pair.unwrap();
      let x: f32 = target.get("x").unwrap_or(0.0);
      let y: f32 = target.get("y").unwrap_or(0.0);
      let z: f32 = target.get("z").unwrap_or(0.0);
      let radius: f32 = target.get("radius").unwrap_or(0.5);
      let height: f32 = target.get("height").unwrap_or(0.0);

      positions.push(cgmath::Point3::new(x, y, z));
      radii.push(radius);
      heights.push(height);
      active.push(true);
      spawn_times.push(0.0);
      velocities.push(cgmath::Vector3::new(0.0, 0.0, 0.0));
    }

    Self {
      targets: TargetComponents {
        positions,
        velocities,
        radii,
        heights,
        active,
        spawn_times,
      },
      timer: 0.0,
      score: 0,
      stats: TrackingStats {
        frames_tracking: 0,
        frames_on_target: 0,
      },
      firing: false,
      name,
      lua,
    }
  }

  /// Ray-capsule test used for tracking accuracy each frame.
  /// A capsule is a vertical pill: sphere-swept line segment along Y.
  /// When height <= 2*radius it degenerates to a sphere test.
  /// Returns true if the crosshair hits any active target.
  pub fn crosshair_on_target(
    &self,
    origin: cgmath::Point3<f32>,
    direction: cgmath::Vector3<f32>,
  ) -> bool {
    let dir = direction.normalize();
    for i in 0..self.targets.positions.len() {
      if !self.targets.active[i] {
        continue;
      }
      let center = self.targets.positions[i];
      let radius = self.targets.radii[i];
      let half_h = (self.targets.heights[i] * 0.5 - radius).max(0.0);

      if Self::ray_capsule_hit(origin, dir, center, radius, half_h) {
        return true;
      }
    }
    false
  }

  /// Test a ray against a vertical capsule (pill).
  /// The capsule is defined by a center point, radius, and half_height of the
  /// cylindrical section.  The total height is `2 * (half_h + radius)`.
  fn ray_capsule_hit(
    origin: cgmath::Point3<f32>,
    dir: cgmath::Vector3<f32>,
    center: cgmath::Point3<f32>,
    radius: f32,
    half_h: f32,
  ) -> bool {
    let r2 = radius * radius;
    let oc = origin - center;

    // --- Cylinder body (infinite cylinder along Y, then clamp) -----------
    let a = dir.x * dir.x + dir.z * dir.z;
    let b = 2.0 * (oc.x * dir.x + oc.z * dir.z);
    let c = oc.x * oc.x + oc.z * oc.z - r2;

    if a > 1e-8 {
      let disc = b * b - 4.0 * a * c;
      if disc >= 0.0 {
        let sqrt_disc = disc.sqrt();
        // Check both intersections (near and far)
        for &t in &[(-b - sqrt_disc) / (2.0 * a), (-b + sqrt_disc) / (2.0 * a)] {
          if t > 0.0 {
            let hit_y = oc.y + t * dir.y;
            if hit_y >= -half_h && hit_y <= half_h {
              return true;
            }
          }
        }
      }
    }

    // --- Top hemisphere cap (sphere at center + half_h * Y) ---------------
    {
      let cap_oc_y = oc.y - half_h;
      let a_s = cgmath::dot(dir, dir);
      let b_s = 2.0 * (oc.x * dir.x + cap_oc_y * dir.y + oc.z * dir.z);
      let c_s = oc.x * oc.x + cap_oc_y * cap_oc_y + oc.z * oc.z - r2;
      let disc = b_s * b_s - 4.0 * a_s * c_s;
      if disc >= 0.0 {
        let sqrt_disc = disc.sqrt();
        for &t in &[
          (-b_s - sqrt_disc) / (2.0 * a_s),
          (-b_s + sqrt_disc) / (2.0 * a_s),
        ] {
          if t > 0.0 {
            let hit_y = oc.y + t * dir.y;
            if hit_y >= half_h {
              return true;
            }
          }
        }
      }
    }

    // --- Bottom hemisphere cap (sphere at center - half_h * Y) ------------
    {
      let cap_oc_y = oc.y + half_h;
      let a_s = cgmath::dot(dir, dir);
      let b_s = 2.0 * (oc.x * dir.x + cap_oc_y * dir.y + oc.z * dir.z);
      let c_s = oc.x * oc.x + cap_oc_y * cap_oc_y + oc.z * oc.z - r2;
      let disc = b_s * b_s - 4.0 * a_s * c_s;
      if disc >= 0.0 {
        let sqrt_disc = disc.sqrt();
        for &t in &[
          (-b_s - sqrt_disc) / (2.0 * a_s),
          (-b_s + sqrt_disc) / (2.0 * a_s),
        ] {
          if t > 0.0 {
            let hit_y = oc.y + t * dir.y;
            if hit_y <= -half_h {
              return true;
            }
          }
        }
      }
    }

    false
  }

  /// Returns true if crosshair is on target (kept for API compat).
  pub fn try_shoot(
    &mut self,
    origin: cgmath::Point3<f32>,
    direction: cgmath::Vector3<f32>,
    _frustum: &FrustumParams,
  ) -> bool {
    self.crosshair_on_target(origin, direction)
  }

  pub fn update(&mut self, dt: f32, frustum: &FrustumParams) {
    self.timer += dt;

    // --- call Lua update(targets, ctx) ------------------------------------
    let result: Result<(), LuaError> = (|| {
      let scenario_table: LuaTable = self.lua.globals().get("_scenario")?;
      let update_fn: LuaFunction = scenario_table.get("update")?;

      // Build targets table
      let targets = self.lua.create_table()?;
      for (i, pos) in self.targets.positions.iter().enumerate() {
        let t = self.lua.create_table()?;
        t.set("x", pos.x)?;
        t.set("y", pos.y)?;
        t.set("z", pos.z)?;
        t.set("radius", self.targets.radii[i])?;
        t.set("height", self.targets.heights[i])?;
        t.set("active", self.targets.active[i])?;
        targets.set((i + 1) as i64, t)?;
      }

      // Build context
      let ctx = self.lua.create_table()?;
      ctx.set("timer", self.timer)?;
      ctx.set("dt", dt)?;
      let fwd = frustum.forward.normalize();
      ctx.set("eye_x", frustum.eye.x)?;
      ctx.set("eye_y", frustum.eye.y)?;
      ctx.set("eye_z", frustum.eye.z)?;
      ctx.set("forward_x", fwd.x)?;
      ctx.set("forward_y", fwd.y)?;
      ctx.set("forward_z", fwd.z)?;

      let result: LuaTable = update_fn.call((targets, ctx))?;

      // Apply updated positions
      for pair in result.pairs::<i64, LuaTable>() {
        let (idx, target) = pair?;
        let i = (idx - 1) as usize;
        if i < self.targets.positions.len() {
          self.targets.positions[i].x = target.get("x")?;
          self.targets.positions[i].y = target.get("y")?;
          self.targets.positions[i].z = target.get("z")?;
          if let Ok(r) = target.get::<f32>("radius") {
            self.targets.radii[i] = r;
          }
          if let Ok(h) = target.get::<f32>("height") {
            self.targets.heights[i] = h;
          }
          if let Ok(a) = target.get::<bool>("active") {
            self.targets.active[i] = a;
          }
        }
      }

      Ok(())
    })();

    if let Err(e) = result {
      eprintln!("[lua] update error: {}", e);
    }

    // Track accuracy while firing (stays in Rust for precision)
    if self.firing {
      self.stats.frames_tracking += 1;
      if self.crosshair_on_target(frustum.eye, frustum.forward) {
        self.stats.frames_on_target += 1;
      }
    }
  }
}
