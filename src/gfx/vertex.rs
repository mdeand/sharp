use egui_wgpu::wgpu;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
  pub position: [f32; 3],
  pub color: [f32; 3],
}

impl Vertex {
  pub fn desc() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
      array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &[
        wgpu::VertexAttribute {
          offset: 0,
          shader_location: 0,
          format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
          offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
          shader_location: 1,
          format: wgpu::VertexFormat::Float32x3,
        },
      ],
    }
  }
}

/// Per-instance data sent to the GPU for instanced rendering.
/// Contains the world-space position, uniform scale (radius), and half-height
/// of the cylindrical section for capsule/pill shapes. When `half_height` is 0
/// the shape degenerates to a sphere.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
  pub position: [f32; 3],
  pub scale: f32,
  pub half_height: f32,
  pub _pad: [f32; 3], // pad to 32 bytes for alignment
}

impl InstanceRaw {
  pub fn desc() -> wgpu::VertexBufferLayout<'static> {
    use std::mem;
    wgpu::VertexBufferLayout {
      array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Instance,
      attributes: &[
        // instance position -> location 2
        wgpu::VertexAttribute {
          offset: 0,
          shader_location: 2,
          format: wgpu::VertexFormat::Float32x3,
        },
        // instance scale (radius) -> location 3
        wgpu::VertexAttribute {
          offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
          shader_location: 3,
          format: wgpu::VertexFormat::Float32,
        },
        // instance half_height (capsule body) -> location 4
        wgpu::VertexAttribute {
          offset: (mem::size_of::<[f32; 3]>() + mem::size_of::<f32>()) as wgpu::BufferAddress,
          shader_location: 4,
          format: wgpu::VertexFormat::Float32,
        },
      ],
    }
  }
}
