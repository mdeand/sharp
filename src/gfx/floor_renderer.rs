use crate::gfx::camera::{Camera, CameraUniform};
use crate::gfx::texture::GpuTexture;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

/// A simple vertex with 3D position and 2D UV.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FloorVertex {
  position: [f32; 3],
  uv: [f32; 2],
}

/// Renders an infinite-looking textured floor plane on the XZ plane at y = 0.
pub struct FloorRenderer {
  render_pipeline: wgpu::RenderPipeline,
  vertex_buffer: wgpu::Buffer,
  vertex_count: u32,
  camera_uniform: CameraUniform,
  camera_buffer: wgpu::Buffer,
  camera_bind_group: wgpu::BindGroup,
  texture_bind_group: wgpu::BindGroup,
  #[allow(dead_code)]
  texture: GpuTexture,
}

impl FloorRenderer {
  /// Create from raw image bytes (PNG/JPEG).
  pub fn from_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    camera: &Camera,
    texture_bytes: &[u8],
  ) -> Self {
    let texture = GpuTexture::from_bytes(device, queue, texture_bytes, "floor_texture");
    Self::new(device, surface_format, camera, texture)
  }

  /// Create from a pre-built `GpuTexture`.
  pub fn new(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera: &Camera,
    texture: GpuTexture,
  ) -> Self {
    // --- Camera uniform ---
    let mut camera_uniform = CameraUniform::new();
    camera_uniform.update_view_proj(camera);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Floor Camera Buffer"),
      contents: bytemuck::cast_slice(&[camera_uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_bind_group_layout =
      device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("floor_camera_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::VERTEX,
          ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
          },
          count: None,
        }],
      });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("floor_camera_bg"),
      layout: &camera_bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: camera_buffer.as_entire_binding(),
      }],
    });

    // --- Texture bind group ---
    let texture_bind_group_layout = GpuTexture::bind_group_layout(device);
    let texture_bind_group = texture.bind_group(device, &texture_bind_group_layout);

    // --- Pipeline ---
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("floor shader"),
      source: wgpu::ShaderSource::Wgsl(include_str!("shaders/floor.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("Floor Pipeline Layout"),
      bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
      push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("floor render pipeline"),
      layout: Some(&pipeline_layout),
      cache: None,
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[wgpu::VertexBufferLayout {
          array_stride: std::mem::size_of::<FloorVertex>() as wgpu::BufferAddress,
          step_mode: wgpu::VertexStepMode::Vertex,
          attributes: &[
            wgpu::VertexAttribute {
              offset: 0,
              shader_location: 0,
              format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
              offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
              shader_location: 1,
              format: wgpu::VertexFormat::Float32x2,
            },
          ],
        }],
        compilation_options: Default::default(),
      },
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        cull_mode: None, // visible from both sides
        ..Default::default()
      },
      depth_stencil: None,
      multisample: wgpu::MultisampleState::default(),
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fs_main"),
        targets: &[Some(wgpu::ColorTargetState {
          format: surface_format,
          blend: Some(wgpu::BlendState::REPLACE),
          write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      multiview: None,
    });

    // --- Geometry: large quad centered at origin, y = 0 ---
    let half = 50.0_f32;
    let uv_scale = 25.0_f32; // texture tiles per half-extent

    #[rustfmt::skip]
    let vertices = [
      FloorVertex { position: [-half, 0.0, -half], uv: [0.0,      0.0] },
      FloorVertex { position: [ half, 0.0, -half], uv: [uv_scale, 0.0] },
      FloorVertex { position: [ half, 0.0,  half], uv: [uv_scale, uv_scale] },
      FloorVertex { position: [-half, 0.0, -half], uv: [0.0,      0.0] },
      FloorVertex { position: [ half, 0.0,  half], uv: [uv_scale, uv_scale] },
      FloorVertex { position: [-half, 0.0,  half], uv: [0.0,      uv_scale] },
    ];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Floor Vertex Buffer"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });

    Self {
      render_pipeline,
      vertex_buffer,
      vertex_count: vertices.len() as u32,
      camera_uniform,
      camera_buffer,
      camera_bind_group,
      texture_bind_group,
      texture,
    }
  }

  pub fn update_camera(&mut self, queue: &wgpu::Queue, camera: &Camera) {
    self.camera_uniform.update_view_proj(camera);
    queue.write_buffer(
      &self.camera_buffer,
      0,
      bytemuck::cast_slice(&[self.camera_uniform]),
    );
  }

  pub fn render(&self, encoder: &mut wgpu::CommandEncoder, surface_view: &wgpu::TextureView) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("floor render pass"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: surface_view,
        resolve_target: None,
        ops: wgpu::Operations {
          load: wgpu::LoadOp::Load,
          store: wgpu::StoreOp::Store,
        },
        depth_slice: None,
      })],
      depth_stencil_attachment: None,
      timestamp_writes: None,
      occlusion_query_set: None,
    });

    rpass.set_pipeline(&self.render_pipeline);
    rpass.set_bind_group(0, &self.camera_bind_group, &[]);
    rpass.set_bind_group(1, &self.texture_bind_group, &[]);
    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
    rpass.draw(0..self.vertex_count, 0..1);
  }
}
