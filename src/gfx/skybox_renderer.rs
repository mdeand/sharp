use crate::gfx::camera::Camera;
use crate::gfx::texture::GpuTexture;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

/// Uniform for the inverse view-projection matrix (used to reconstruct world-space ray dirs).
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyboxUniform {
  inv_view_proj: [[f32; 4]; 4],
}

/// Renders a cubemap sky using a fullscreen triangle + inverse VP matrix.
pub struct SkyboxRenderer {
  render_pipeline: wgpu::RenderPipeline,
  uniform: SkyboxUniform,
  uniform_buffer: wgpu::Buffer,
  camera_bind_group: wgpu::BindGroup,
  texture_bind_group: wgpu::BindGroup,
  #[allow(dead_code)]
  texture: GpuTexture,
}

impl SkyboxRenderer {
  /// `faces` must be 6 image byte slices in order: +X, -X, +Y, -Y, +Z, -Z.
  pub fn from_faces(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    camera: &Camera,
    faces: &[&[u8]; 6],
  ) -> Self {
    let texture = GpuTexture::from_cubemap_bytes(device, queue, faces, "skybox_cubemap");
    Self::new(device, surface_format, camera, texture)
  }

  /// Create a skybox renderer from a pre-built cubemap `GpuTexture`.
  pub fn new(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    camera: &Camera,
    texture: GpuTexture,
  ) -> Self {
    // --- Uniform ---
    let inv_vp = camera.build_skybox_inv_vp();
    let uniform = SkyboxUniform {
      inv_view_proj: inv_vp.into(),
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Skybox Uniform Buffer"),
      contents: bytemuck::cast_slice(&[uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_bind_group_layout =
      device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("skybox_camera_bgl"),
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
      label: Some("skybox_camera_bg"),
      layout: &camera_bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });

    // --- Cubemap texture bind group ---
    let texture_bind_group_layout = GpuTexture::cubemap_bind_group_layout(device);
    let texture_bind_group = texture.bind_group(device, &texture_bind_group_layout);

    // --- Pipeline ---
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("skybox shader"),
      source: wgpu::ShaderSource::Wgsl(include_str!("shaders/skybox.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("Skybox Pipeline Layout"),
      bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
      push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("skybox render pipeline"),
      layout: Some(&pipeline_layout),
      cache: None,
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[], // fullscreen triangle generated in shader
        compilation_options: Default::default(),
      },
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        cull_mode: None,
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

    Self {
      render_pipeline,
      uniform,
      uniform_buffer,
      camera_bind_group,
      texture_bind_group,
      texture,
    }
  }

  pub fn update_camera(&mut self, queue: &wgpu::Queue, camera: &Camera) {
    let inv_vp = camera.build_skybox_inv_vp();
    self.uniform.inv_view_proj = inv_vp.into();
    queue.write_buffer(
      &self.uniform_buffer,
      0,
      bytemuck::cast_slice(&[self.uniform]),
    );
  }

  /// Render the skybox. Should be called *first* (before the scene) to fill the background.
  pub fn render(&self, encoder: &mut wgpu::CommandEncoder, surface_view: &wgpu::TextureView) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("skybox render pass"),
      color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        view: surface_view,
        resolve_target: None,
        ops: wgpu::Operations {
          load: wgpu::LoadOp::Clear(wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
          }),
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
    rpass.draw(0..3, 0..1); // fullscreen triangle (3 vertices, generated in shader)
  }
}
