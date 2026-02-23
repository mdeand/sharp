use std::{cell::OnceCell, io::Cursor, path::Path, sync::Arc, time::Instant};

use cgmath::InnerSpace;
use egui_wgpu::wgpu;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Source};

use crate::gfx::{
  camera::{Camera, CameraController},
  crosshair_renderer::CrosshairRenderer,
  egui_renderer::EguiRenderer,
  floor_renderer::FloorRenderer,
  scene_renderer::SceneRenderer,
  skybox_renderer::SkyboxRenderer,
  texture::GpuTexture,
};
use crate::scenario::{FrustumParams, Scenario};

/// Raw bytes of the hit sound, embedded at compile time.
const HIT_SOUND: &[u8] = include_bytes!("../assets/blip 2.ogg");

pub struct AppState {
  device: wgpu::Device,
  queue: wgpu::Queue,
  surface_config: wgpu::SurfaceConfiguration,
  surface: wgpu::Surface<'static>,
  egui_renderer: EguiRenderer,
  scene_renderer: SceneRenderer,
  crosshair_renderer: CrosshairRenderer,
  floor_renderer: FloorRenderer,
  skybox_renderer: SkyboxRenderer,
  camera: Camera,
  camera_controller: CameraController,
  scenario: Scenario,
  last_frame: Instant,
  fps: f32,
  #[allow(dead_code)]
  audio_stream: OutputStream,
  audio_handle: OutputStreamHandle,
  /// Time of last hit sound play.
  last_hit_sound: Instant,
}

impl AppState {
  async fn new(
    instance: &wgpu::Instance,
    surface: wgpu::Surface<'static>,
    window: &winit::window::Window,
    width: u32,
    height: u32,
  ) -> Self {
    let adapter = instance
      .request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
      })
      .await
      .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
      .request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
      })
      .await
      .expect("Failed to create device");

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
      .formats
      .iter()
      .find(|d| **d == wgpu::TextureFormat::Bgra8UnormSrgb)
      .expect("Failed to find proper surface format");

    let surface_config = wgpu::SurfaceConfiguration {
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
      format: *surface_format,
      width,
      height,
      present_mode: wgpu::PresentMode::AutoVsync,
      desired_maximum_frame_latency: 0,
      alpha_mode: surface_caps.alpha_modes[0],
      view_formats: vec![],
    };

    surface.configure(&device, &surface_config);

    let egui_renderer = EguiRenderer::new(&device, surface_config.format, None, 1, window);

    let camera = Camera {
      eye: (0.0, 1.0, 2.0).into(),
      target: (0.0, 0.0, 0.0).into(),
      up: cgmath::Vector3::unit_y(),
      aspect: width as f32 / height as f32,
      fovy: 90.0,
      znear: 0.1,
      zfar: 100.0,
    };

    let scene_renderer = SceneRenderer::new(&device, surface_config.format, &camera);
    let crosshair_renderer = CrosshairRenderer::new(
      &device,
      &queue,
      surface_config.format,
      surface_config.width,
      surface_config.height,
    );

    // Floor — use a texture file if available, otherwise a procedural checkerboard
    let floor_renderer = if let Ok(bytes) = std::fs::read("assets/floor.png") {
      FloorRenderer::from_bytes(&device, &queue, surface_config.format, &camera, &bytes)
    } else if let Ok(bytes) = std::fs::read("assets/floor.jpg") {
      FloorRenderer::from_bytes(&device, &queue, surface_config.format, &camera, &bytes)
    } else {
      let tex = GpuTexture::checkerboard(
        &device,
        &queue,
        256,
        32,
        [180, 180, 180, 255],
        [100, 100, 100, 255],
      );
      FloorRenderer::new(&device, surface_config.format, &camera, tex)
    };

    // Skybox — use 6 face images if available, otherwise a procedural placeholder
    let skybox_renderer = {
      let face_names = [
        "assets/skybox_px.png",
        "assets/skybox_nx.png",
        "assets/skybox_py.png",
        "assets/skybox_ny.png",
        "assets/skybox_pz.png",
        "assets/skybox_nz.png",
      ];
      let face_data: Vec<Option<Vec<u8>>> = face_names
        .iter()
        .map(|name| std::fs::read(name).ok())
        .collect();

      if face_data.iter().all(|f| f.is_some()) {
        let refs: Vec<&[u8]> = face_data.iter().map(|f| f.as_deref().unwrap()).collect();
        let faces: [&[u8]; 6] = [refs[0], refs[1], refs[2], refs[3], refs[4], refs[5]];
        SkyboxRenderer::from_faces(&device, &queue, surface_config.format, &camera, &faces)
      } else {
        let tex = GpuTexture::placeholder_cubemap(&device, &queue, 64);
        SkyboxRenderer::new(&device, surface_config.format, &camera, tex)
      }
    };

    let frustum = FrustumParams {
      eye: camera.eye,
      forward: cgmath::Vector3::new(0.0, 0.0, -1.0),
      fovy_deg: camera.fovy,
      aspect: camera.aspect,
    };
    let scenario = Scenario::load(Path::new("scenarios/smoothbot_invincible.lua"), &frustum);

    let (audio_stream, audio_handle) =
      OutputStream::try_default().expect("Failed to open audio output");

    Self {
      device,
      queue,
      surface_config,
      surface,
      egui_renderer,
      scene_renderer,
      crosshair_renderer,
      floor_renderer,
      skybox_renderer,
      camera,
      camera_controller: CameraController::new(1.0, 32.0, 1000.0),
      scenario,
      last_frame: Instant::now(),
      fps: 0.0,
      audio_stream,
      audio_handle,
      last_hit_sound: Instant::now(),
    }
  }

  fn resize_surface(&mut self, new_width: u32, new_height: u32) {
    if new_width > 0 && new_height > 0 {
      self.surface_config.width = new_width;
      self.surface_config.height = new_height;

      self.camera.aspect = new_width as f32 / new_height as f32;

      self.surface.configure(&self.device, &self.surface_config);

      self.scene_renderer.update_camera(&self.queue, &self.camera);
      self.floor_renderer.update_camera(&self.queue, &self.camera);
      self
        .skybox_renderer
        .update_camera(&self.queue, &self.camera);

      self
        .crosshair_renderer
        .resize(&self.device, new_width, new_height);
    }
  }
}

pub struct App {
  instance: wgpu::Instance,
  state: OnceCell<AppState>,
  window: OnceCell<Arc<winit::window::Window>>,
}

impl App {
  pub fn new() -> Self {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
      backends: wgpu::Backends::all(),
      ..Default::default()
    });

    Self {
      instance,
      state: OnceCell::new(),
      window: OnceCell::new(),
    }
  }

  async fn set_window(&self, window: winit::window::Window) {
    let window = Arc::new(window);

    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));

    let size = window.inner_size();
    let initial_width = size.width.max(1);
    let initial_height = size.height.max(1);

    window
      .set_cursor_grab(winit::window::CursorGrabMode::Locked)
      .expect("Failed to grab cursor");
    window.set_cursor_visible(false);

    let surface = self
      .instance
      .create_surface(window.clone())
      .expect("Failed to create surface!");

    let state = AppState::new(
      &self.instance,
      surface,
      &window,
      initial_width,
      initial_height,
    )
    .await;

    if let Err(_) = self.window.set(window) {
      panic!("Window already set!");
    }

    if let Err(_) = self.state.set(state) {
      panic!("AppState already set!");
    }
  }

  fn handle_resize(&mut self, new_width: u32, new_height: u32) {
    if new_width > 0 && new_height > 0 {
      self
        .state
        .get_mut()
        .expect("AppState not initialized!")
        .resize_surface(new_width, new_height);
    }
  }

  fn handle_redraw(&mut self) {
    let window = match self.window.get() {
      Some(window) if window.is_minimized().unwrap_or(false) => return,
      Some(window) => window,
      None => return,
    };

    let state = self.state.get_mut().expect("AppState not initialized!");

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
      size_in_pixels: [state.surface_config.width, state.surface_config.height],
      pixels_per_point: window.scale_factor() as f32,
    };

    let surface_texture = match state.surface.get_current_texture() {
      Err(wgpu::SurfaceError::Outdated) => return,
      Err(e) => panic!("Failed to acquire next swap chain texture: {:?}", e),
      Ok(surface_texture) => surface_texture,
    };

    let surface_view = surface_texture
      .texture
      .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = state
      .device
      .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    state
      .scene_renderer
      .update_camera(&state.queue, &state.camera);

    state
      .floor_renderer
      .update_camera(&state.queue, &state.camera);

    state
      .skybox_renderer
      .update_camera(&state.queue, &state.camera);

    state
      .scene_renderer
      .update_instances(&state.queue, &state.scenario);

    // Update scenario (lifespan, movement, respawning)
    {
      let now = Instant::now();
      let dt = now.duration_since(state.last_frame).as_secs_f32();
      state.last_frame = now;
      // Smooth FPS with exponential moving average
      let instant_fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
      state.fps = state.fps * 0.95 + instant_fps * 0.05;

      let frustum = FrustumParams {
        eye: state.camera.eye,
        forward: (state.camera.target - state.camera.eye).normalize(),
        fovy_deg: state.camera.fovy,
        aspect: state.camera.aspect,
      };
      state.scenario.update(dt, &frustum);
    }

    // Play hit sound every 0.05s while firing and on target.
    // Decode on a background thread so the OGG parse doesn't stall the frame.
    if state.scenario.firing {
      let forward = (state.camera.target - state.camera.eye).normalize();
      let on_target = state
        .scenario
        .crosshair_on_target(state.camera.eye, forward);
      if on_target {
        let elapsed = state.last_hit_sound.elapsed().as_secs_f32();
        if elapsed >= 0.05 {
          state.last_hit_sound = Instant::now();
          let sink = state.audio_handle.clone();
          std::thread::spawn(move || {
            if let Ok(source) = Decoder::new(Cursor::new(HIT_SOUND)) {
              let _ = sink.play_raw(source.convert_samples());
            }
          });
        }
      }
    }

    // Render order: skybox (background) → floor → scene (spheres) → crosshair → egui
    state.skybox_renderer.render(&mut encoder, &surface_view);
    state.floor_renderer.render(&mut encoder, &surface_view);
    state.scene_renderer.render(&mut encoder, &surface_view);
    state.crosshair_renderer.render(&mut encoder, &surface_view);

    let window = self.window.get().expect("Window not initialized!");

    {
      state.egui_renderer.begin_frame(window);

      egui::Window::new(&state.scenario.name)
        .resizable(true)
        .vscroll(true)
        .default_open(true)
        .show(state.egui_renderer.context(), |ui| {
          let acc = state.scenario.stats.accuracy_pct();
          ui.label(format!("Accuracy: {:.1}%", acc));

          ui.label(format!(
            "On target: {} / {}",
            state.scenario.stats.frames_on_target, state.scenario.stats.frames_tracking,
          ));

          ui.label(format!("Time: {:.1}s", state.scenario.timer,));

          ui.label(format!(
            "Firing: {}",
            if state.scenario.firing { "YES" } else { "---" },
          ));

          ui.label(format!("FPS: {:.0}", state.fps));
        });

      state.egui_renderer.end_frame_and_draw(
        &state.device,
        &state.queue,
        &mut encoder,
        window,
        &surface_view,
        screen_descriptor,
      );
    }

    state.queue.submit(Some(encoder.finish()));
    surface_texture.present();
  }
}

impl winit::application::ApplicationHandler for App {
  fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
    let window = event_loop
      .create_window(
        winit::window::Window::default_attributes()
          .with_title(format!("Sharp v{}", env!("CARGO_PKG_VERSION"))),
      )
      .unwrap();

    pollster::block_on(self.set_window(window));
  }

  fn window_event(
    &mut self,
    event_loop: &winit::event_loop::ActiveEventLoop,
    _: winit::window::WindowId,
    event: winit::event::WindowEvent,
  ) {
    use winit::event::WindowEvent;

    self
      .state
      .get_mut()
      .expect("AppState not initialized!")
      .egui_renderer
      .handle_input(self.window.get().expect("Window not initialized!"), &event);

    match event {
      WindowEvent::CloseRequested => {
        event_loop.exit();
      }
      WindowEvent::MouseInput {
        state: winit::event::ElementState::Pressed,
        button: winit::event::MouseButton::Left,
        ..
      } => {
        let app_state = self.state.get_mut().expect("AppState not initialized!");
        app_state.scenario.firing = true;
      }
      WindowEvent::MouseInput {
        state: winit::event::ElementState::Released,
        button: winit::event::MouseButton::Left,
        ..
      } => {
        let app_state = self.state.get_mut().expect("AppState not initialized!");
        app_state.scenario.firing = false;
      }
      WindowEvent::RedrawRequested => {
        self.handle_redraw();
        self
          .window
          .get()
          .expect("Window not initialized!")
          .request_redraw();
      }
      WindowEvent::Resized(new_size) => {
        self.handle_resize(new_size.width, new_size.height);
      }
      WindowEvent::Focused(true) => {
        let window = self.window.get().expect("Window not initialized!");
        // Re-center and re-lock the cursor when the window regains focus
        let size = window.inner_size();
        let center =
          winit::dpi::PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0);
        let _ = window.set_cursor_position(center);
        let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Locked);
        window.set_cursor_visible(false);
      }
      _ => (),
    }
  }

  fn device_event(
    &mut self,
    _event_loop: &winit::event_loop::ActiveEventLoop,
    _device_id: winit::event::DeviceId,
    event: winit::event::DeviceEvent,
  ) {
    if let winit::event::DeviceEvent::Motion { axis, value } = event {
      let state = self.state.get_mut().unwrap();

      match axis {
        0 => state.camera_controller.process_mouse(value, 0.0),
        1 => state.camera_controller.process_mouse(0.0, value),
        _ => (),
      }

      state.camera_controller.update_camera(&mut state.camera);
    }
  }
}
