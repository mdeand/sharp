use crate::gfx::camera::{Camera, CameraUniform};
use crate::gfx::mesh::create_sphere;
use crate::gfx::vertex::{InstanceRaw, Vertex};
use crate::scenario::Scenario;
use egui_wgpu::wgpu::util::DeviceExt;
use egui_wgpu::wgpu::{self, Buffer, Device, RenderPipeline, TextureView};

/// Maximum number of sphere instances we can render in a single draw call.
const MAX_INSTANCES: usize = 1024;

pub struct SceneRenderer {
  render_pipeline: RenderPipeline,
  vertex_buffer: Buffer,
  index_buffer: Buffer,
  num_indices: u32,
  instance_buffer: Buffer,
  num_instances: u32,
  camera_uniform: CameraUniform,
  camera_buffer: Buffer,
  camera_bind_group: wgpu::BindGroup,
}

impl SceneRenderer {
  pub fn new(device: &Device, surface_format: wgpu::TextureFormat, camera: &Camera) -> Self {
    let mut camera_uniform = CameraUniform::new();
    camera_uniform.update_view_proj(camera);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Camera Buffer"),
      contents: bytemuck::cast_slice(&[camera_uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let camera_bind_group_layout =
      device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        label: Some("camera_bind_group_layout"),
      });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      layout: &camera_bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: camera_buffer.as_entire_binding(),
      }],
      label: Some("camera_bind_group"),
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("scene shader"),
      source: wgpu::ShaderSource::Wgsl(include_str!("shaders/scene.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("Render Pipeline Layout"),
      bind_group_layouts: &[&camera_bind_group_layout],
      push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("scene render pipeline"),
      layout: Some(&pipeline_layout),
      cache: None,
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[Vertex::desc(), InstanceRaw::desc()],
        compilation_options: Default::default(),
      },
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: Some(wgpu::Face::Back),
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
      },
      depth_stencil: None,
      multisample: wgpu::MultisampleState {
        count: 1,
        mask: !0,
        alpha_to_coverage_enabled: false,
      },
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

    // Build a unit-radius icosphere mesh (shared geometry for all sphere instances)
    let (vertices, indices) = create_sphere(2);

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Sphere Vertex Buffer"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Sphere Index Buffer"),
      contents: bytemuck::cast_slice(&indices),
      usage: wgpu::BufferUsages::INDEX,
    });

    let num_indices = indices.len() as u32;

    // Pre-allocate instance buffer large enough for MAX_INSTANCES spheres
    let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("Instance Buffer"),
      size: (MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
      usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
    });

    Self {
      render_pipeline,
      vertex_buffer,
      index_buffer,
      num_indices,
      instance_buffer,
      num_instances: 0,
      camera_uniform,
      camera_buffer,
      camera_bind_group,
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

  /// Upload per-instance data (position + radius) for all active targets.
  pub fn update_instances(&mut self, queue: &wgpu::Queue, scenario: &Scenario) {
    let instances: Vec<InstanceRaw> = scenario
      .targets
      .positions
      .iter()
      .zip(scenario.targets.radii.iter())
      .zip(scenario.targets.heights.iter())
      .zip(scenario.targets.active.iter())
      .filter(|(_, active)| **active)
      .map(|(((pos, &radius), &height), _)| {
        let half_height = (height * 0.5 - radius).max(0.0);
        InstanceRaw {
          position: [pos.x, pos.y, pos.z],
          scale: radius,
          half_height,
          _pad: [0.0; 3],
        }
      })
      .take(MAX_INSTANCES)
      .collect();

    self.num_instances = instances.len() as u32;

    if !instances.is_empty() {
      queue.write_buffer(
        &self.instance_buffer,
        0,
        bytemuck::cast_slice(&instances),
      );
    }
  }

  pub fn render(&self, encoder: &mut wgpu::CommandEncoder, surface_view: &TextureView) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("scene render pass"),
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
    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
    rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
    rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    rpass.draw_indexed(0..self.num_indices, 0, 0..self.num_instances);
  }
}
