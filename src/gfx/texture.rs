use egui_wgpu::wgpu;
use image::GenericImageView;

/// A GPU texture + view + sampler, ready to be bound in a shader.
pub struct GpuTexture {
  pub texture: wgpu::Texture,
  pub view: wgpu::TextureView,
  pub sampler: wgpu::Sampler,
}

impl GpuTexture {
  /// Load a 2D texture from raw file bytes (PNG / JPEG).
  pub fn from_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    label: &str,
  ) -> Self {
    let img = image::load_from_memory(bytes).expect("Failed to decode image");
    let rgba = img.to_rgba8();
    let (width, height) = img.dimensions();
    Self::from_rgba(device, queue, &rgba, width, height, label)
  }

  /// Create a 2D texture from raw RGBA pixel data, with a full mip chain generated on the CPU.
  pub fn from_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    width: u32,
    height: u32,
    label: &str,
  ) -> Self {
    let mip_level_count = (width.max(height) as f32).log2().floor() as u32 + 1;

    let size = wgpu::Extent3d {
      width,
      height,
      depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size,
      mip_level_count,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8UnormSrgb,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    // Upload mip level 0
    queue.write_texture(
      wgpu::TexelCopyTextureInfo {
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
      },
      rgba,
      wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(4 * width),
        rows_per_image: Some(height),
      },
      size,
    );

    // CPU-generate and upload remaining mip levels via simple box filter
    let mut prev = rgba.to_vec();
    let mut mip_w = width;
    let mut mip_h = height;

    for level in 1..mip_level_count {
      let new_w = (mip_w / 2).max(1);
      let new_h = (mip_h / 2).max(1);
      let mut next = vec![0u8; (new_w * new_h * 4) as usize];

      for y in 0..new_h {
        for x in 0..new_w {
          let dst = ((y * new_w + x) * 4) as usize;
          // Average a 2×2 block from the previous level
          let sx = (x * 2).min(mip_w - 1);
          let sy = (y * 2).min(mip_h - 1);
          let sx1 = (sx + 1).min(mip_w - 1);
          let sy1 = (sy + 1).min(mip_h - 1);

          let i00 = ((sy * mip_w + sx) * 4) as usize;
          let i10 = ((sy * mip_w + sx1) * 4) as usize;
          let i01 = ((sy1 * mip_w + sx) * 4) as usize;
          let i11 = ((sy1 * mip_w + sx1) * 4) as usize;

          for c in 0..4 {
            let avg = (prev[i00 + c] as u16
              + prev[i10 + c] as u16
              + prev[i01 + c] as u16
              + prev[i11 + c] as u16)
              / 4;
            next[dst + c] = avg as u8;
          }
        }
      }

      queue.write_texture(
        wgpu::TexelCopyTextureInfo {
          texture: &texture,
          mip_level: level,
          origin: wgpu::Origin3d::ZERO,
          aspect: wgpu::TextureAspect::All,
        },
        &next,
        wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(4 * new_w),
          rows_per_image: Some(new_h),
        },
        wgpu::Extent3d {
          width: new_w,
          height: new_h,
          depth_or_array_layers: 1,
        },
      );

      prev = next;
      mip_w = new_w;
      mip_h = new_h;
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::Repeat,
      address_mode_v: wgpu::AddressMode::Repeat,
      address_mode_w: wgpu::AddressMode::Repeat,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::FilterMode::Linear,
      anisotropy_clamp: 16,
      ..Default::default()
    });

    Self {
      texture,
      view,
      sampler,
    }
  }

  /// Create a bind group layout suitable for a simple 2D textured material
  /// (texture at binding 0, sampler at binding 1).
  pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("texture_bind_group_layout"),
      entries: &[
        wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
          },
          count: None,
        },
        wgpu::BindGroupLayoutEntry {
          binding: 1,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
          count: None,
        },
      ],
    })
  }

  /// Create a bind group for this texture.
  pub fn bind_group(
    &self,
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
  ) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("texture_bind_group"),
      layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: wgpu::BindingResource::TextureView(&self.view),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::Sampler(&self.sampler),
        },
      ],
    })
  }

  /// Load a cubemap from 6 face images (in order: +X, -X, +Y, -Y, +Z, -Z).
  pub fn from_cubemap_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    faces: &[&[u8]; 6],
    label: &str,
  ) -> Self {
    let first = image::load_from_memory(faces[0]).expect("Failed to decode cubemap face");
    let (width, height) = first.dimensions();

    let size = wgpu::Extent3d {
      width,
      height,
      depth_or_array_layers: 6,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8UnormSrgb,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    for (i, face_bytes) in faces.iter().enumerate() {
      let img = image::load_from_memory(face_bytes).expect("Failed to decode cubemap face");
      let rgba = img.to_rgba8();
      queue.write_texture(
        wgpu::TexelCopyTextureInfo {
          texture: &texture,
          mip_level: 0,
          origin: wgpu::Origin3d {
            x: 0,
            y: 0,
            z: i as u32,
          },
          aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(4 * width),
          rows_per_image: Some(height),
        },
        wgpu::Extent3d {
          width,
          height,
          depth_or_array_layers: 1,
        },
      );
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
      dimension: Some(wgpu::TextureViewDimension::Cube),
      ..Default::default()
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::ClampToEdge,
      address_mode_v: wgpu::AddressMode::ClampToEdge,
      address_mode_w: wgpu::AddressMode::ClampToEdge,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::FilterMode::Linear,
      ..Default::default()
    });

    Self {
      texture,
      view,
      sampler,
    }
  }

  /// Create a bind group layout for a cubemap (texture_cube at binding 0, sampler at binding 1).
  pub fn cubemap_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("cubemap_bind_group_layout"),
      entries: &[
        wgpu::BindGroupLayoutEntry {
          binding: 0,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::Cube,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
          },
          count: None,
        },
        wgpu::BindGroupLayoutEntry {
          binding: 1,
          visibility: wgpu::ShaderStages::FRAGMENT,
          ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
          count: None,
        },
      ],
    })
  }

  /// Generate a procedural checkerboard 2D texture (useful as a placeholder floor).
  pub fn checkerboard(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
    tile_size: u32,
    color_a: [u8; 4],
    color_b: [u8; 4],
  ) -> Self {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
      for x in 0..size {
        let checker = ((x / tile_size) + (y / tile_size)) % 2 == 0;
        let c = if checker { color_a } else { color_b };
        let idx = ((y * size + x) * 4) as usize;
        rgba[idx..idx + 4].copy_from_slice(&c);
      }
    }
    Self::from_rgba(device, queue, &rgba, size, size, "checkerboard")
  }

  /// Generate a procedural solid-color cubemap (useful as a placeholder skybox).
  /// Each face gets a slightly different shade so you can tell orientation.
  pub fn placeholder_cubemap(device: &wgpu::Device, queue: &wgpu::Queue, size: u32) -> Self {
    // Face colours: +X red, -X dark red, +Y green, -Y dark green, +Z blue, -Z dark blue
    let face_colors: [[u8; 4]; 6] = [
      [135, 170, 220, 255], // +X  light blue
      [100, 140, 200, 255], // -X
      [160, 200, 240, 255], // +Y  bright sky
      [ 80, 100, 130, 255], // -Y  dark
      [120, 160, 210, 255], // +Z
      [110, 150, 200, 255], // -Z
    ];

    let face_size = (size * size * 4) as usize;
    let mut all_faces: Vec<Vec<u8>> = Vec::with_capacity(6);
    for color in &face_colors {
      let mut face = vec![0u8; face_size];
      for pixel in face.chunks_exact_mut(4) {
        pixel.copy_from_slice(color);
      }
      all_faces.push(face);
    }

    let tex_size = wgpu::Extent3d {
      width: size,
      height: size,
      depth_or_array_layers: 6,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("placeholder_cubemap"),
      size: tex_size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8UnormSrgb,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    for (i, face_data) in all_faces.iter().enumerate() {
      queue.write_texture(
        wgpu::TexelCopyTextureInfo {
          texture: &texture,
          mip_level: 0,
          origin: wgpu::Origin3d {
            x: 0,
            y: 0,
            z: i as u32,
          },
          aspect: wgpu::TextureAspect::All,
        },
        face_data,
        wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(4 * size),
          rows_per_image: Some(size),
        },
        wgpu::Extent3d {
          width: size,
          height: size,
          depth_or_array_layers: 1,
        },
      );
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
      dimension: Some(wgpu::TextureViewDimension::Cube),
      ..Default::default()
    });

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::ClampToEdge,
      address_mode_v: wgpu::AddressMode::ClampToEdge,
      address_mode_w: wgpu::AddressMode::ClampToEdge,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      ..Default::default()
    });

    Self {
      texture,
      view,
      sampler,
    }
  }
}
