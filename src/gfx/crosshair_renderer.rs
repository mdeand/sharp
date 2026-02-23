use crate::gfx::texture::GpuTexture;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
use image::GenericImageView;

const CROSSHAIR_IMAGE: &[u8] = include_bytes!("../../assets/ZeeqPlus1.png");

/// Renders a textured crosshair image at the center of the screen.
pub struct CrosshairRenderer {
  render_pipeline: wgpu::RenderPipeline,
  vertex_buffer: wgpu::Buffer,
  texture_bind_group: wgpu::BindGroup,
  #[allow(dead_code)]
  texture: GpuTexture,
  /// Native pixel dimensions of the crosshair image.
  img_width: u32,
  img_height: u32,
}

/// 2D vertex with position (NDC) and UV.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CrosshairVertex {
  position: [f32; 2],
  uv: [f32; 2],
}

impl CrosshairRenderer {
  pub fn new(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    screen_width: u32,
    screen_height: u32,
  ) -> Self {
    let img = image::load_from_memory(CROSSHAIR_IMAGE).expect("Failed to decode crosshair image");
    let (img_width, img_height) = img.dimensions();

    let texture = GpuTexture::from_bytes(device, queue, CROSSHAIR_IMAGE, "crosshair_texture");
    let texture_bind_group_layout = GpuTexture::bind_group_layout(device);
    let texture_bind_group = texture.bind_group(device, &texture_bind_group_layout);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("crosshair shader"),
      source: wgpu::ShaderSource::Wgsl(include_str!("shaders/crosshair.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
      label: Some("Crosshair Pipeline Layout"),
      bind_group_layouts: &[&texture_bind_group_layout],
      push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
      label: Some("crosshair render pipeline"),
      layout: Some(&pipeline_layout),
      cache: None,
      vertex: wgpu::VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[wgpu::VertexBufferLayout {
          array_stride: std::mem::size_of::<CrosshairVertex>() as wgpu::BufferAddress,
          step_mode: wgpu::VertexStepMode::Vertex,
          attributes: &[
            wgpu::VertexAttribute {
              offset: 0,
              shader_location: 0,
              format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
              offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
              shader_location: 1,
              format: wgpu::VertexFormat::Float32x2,
            },
          ],
        }],
        compilation_options: Default::default(),
      },
      primitive: wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        ..Default::default()
      },
      depth_stencil: None,
      multisample: wgpu::MultisampleState::default(),
      fragment: Some(wgpu::FragmentState {
        module: &shader,
        entry_point: Some("fs_main"),
        targets: &[Some(wgpu::ColorTargetState {
          format: surface_format,
          blend: Some(wgpu::BlendState::ALPHA_BLENDING),
          write_mask: wgpu::ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      multiview: None,
    });

    let vertices = Self::build_vertices(img_width, img_height, screen_width, screen_height);

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Crosshair Vertex Buffer"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });

    Self {
      render_pipeline,
      vertex_buffer,
      texture_bind_group,
      texture,
      img_width,
      img_height,
    }
  }

  /// Rebuild the vertex buffer when the screen size changes so the image
  /// retains its native pixel dimensions.
  pub fn resize(&mut self, device: &wgpu::Device, screen_width: u32, screen_height: u32) {
    let vertices =
      Self::build_vertices(self.img_width, self.img_height, screen_width, screen_height);
    self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Crosshair Vertex Buffer"),
      contents: bytemuck::cast_slice(&vertices),
      usage: wgpu::BufferUsages::VERTEX,
    });
  }

  /// Compute a centred quad in NDC that maps 1:1 to the image's pixel size on
  /// the current screen.
  fn build_vertices(img_w: u32, img_h: u32, screen_w: u32, screen_h: u32) -> [CrosshairVertex; 6] {
    // NDC goes from -1..1, so full width = 2 NDC units = screen_w pixels.
    let half_x = img_w as f32 / screen_w as f32; // in NDC
    let half_y = img_h as f32 / screen_h as f32; // in NDC

    #[rustfmt::skip]
    let verts = [
      CrosshairVertex { position: [-half_x, -half_y], uv: [0.0, 1.0] },
      CrosshairVertex { position: [ half_x, -half_y], uv: [1.0, 1.0] },
      CrosshairVertex { position: [ half_x,  half_y], uv: [1.0, 0.0] },
      CrosshairVertex { position: [-half_x, -half_y], uv: [0.0, 1.0] },
      CrosshairVertex { position: [ half_x,  half_y], uv: [1.0, 0.0] },
      CrosshairVertex { position: [-half_x,  half_y], uv: [0.0, 0.0] },
    ];
    verts
  }

  /// Draw the crosshair. Call this *after* the scene pass but *before* the egui pass.
  pub fn render(&self, encoder: &mut wgpu::CommandEncoder, surface_view: &wgpu::TextureView) {
    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
      label: Some("crosshair render pass"),
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
    rpass.set_bind_group(0, &self.texture_bind_group, &[]);
    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
    rpass.draw(0..6, 0..1);
  }
}
