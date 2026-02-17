use cgmath::*;

pub struct Camera {
  pub eye: Point3<f32>,
  pub target: Point3<f32>,
  pub up: Vector3<f32>,
  pub aspect: f32,
  pub fovy: f32,
  pub znear: f32,
  pub zfar: f32,
}

impl Camera {
  pub fn build_view_projection_matrix(&self) -> Matrix4<f32> {
    let view = Matrix4::look_at_rh(self.eye, self.target, self.up);
    let proj = perspective(Deg(self.fovy), self.aspect, self.znear, self.zfar);

    let correction = Matrix4::new(
      1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 1.0,
    );

    correction * proj * view
  }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
  view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
  pub fn new() -> Self {
    Self {
      view_proj: cgmath::Matrix4::identity().into(),
    }
  }

  pub fn update_view_proj(&mut self, camera: &Camera) {
    self.view_proj = camera.build_view_projection_matrix().into();
  }
}

pub struct CameraController {
  speed: f32,
  pub radians_per_dot: f32,
  forward: f32,
  backward: f32,
  left: f32,
  right: f32,
  pub yaw: Rad<f32>,
  pub pitch: Rad<f32>,
}

impl CameraController {
  pub fn new(speed: f32, cm_360: f32, mouse_dpi: f32) -> Self {
    // (tau * CM per Inch) / (Target CM * DPI)
    let radians_per_dot = (2.0 * std::f32::consts::PI * 2.54) / (cm_360 * mouse_dpi);

    Self {
      speed,
      radians_per_dot,
      forward: 0.0,
      backward: 0.0,
      left: 0.0,
      right: 0.0,
      yaw: Rad(-std::f32::consts::FRAC_PI_2),
      pitch: Rad(0.0),
    }
  }

  pub fn set_sensitivity(&mut self, cm_360: f32, mouse_dpi: f32) {
    self.radians_per_dot = (2.0 * std::f32::consts::PI * 2.54) / (cm_360 * mouse_dpi);
  }

  pub fn process_mouse(&mut self, mouse_dx: f64, mouse_dy: f64) {
    self.yaw += Rad(mouse_dx as f32) * self.radians_per_dot;
    self.pitch -= Rad(mouse_dy as f32) * self.radians_per_dot;

    // NOTE(mdeand): Clamp pitch to prevent backflips
    let safe_pitch = Rad(std::f32::consts::FRAC_PI_2 - 0.001);
    if self.pitch < -safe_pitch {
      self.pitch = -safe_pitch;
    } else if self.pitch > safe_pitch {
      self.pitch = safe_pitch;
    }
  }

  pub fn update_camera(&self, camera: &mut Camera) {
    let forward = Vector3::new(
      self.yaw.0.cos() * self.pitch.0.cos(),
      self.pitch.0.sin(),
      self.yaw.0.sin() * self.pitch.0.cos(),
    )
    .normalize();

    let right = forward.cross(Vector3::unit_y()).normalize();

    camera.eye += forward * (self.forward - self.backward) * self.speed;
    camera.eye += right * (self.right - self.left) * self.speed;

    camera.target = camera.eye + forward;
  }
}
