use std::{cell::OnceCell, sync::Arc};

use egui_wgpu::wgpu;

use crate::render::{egui_renderer::EguiRenderer, scene_renderer::SceneRenderer};

pub struct AppState {
  device: wgpu::Device,
  queue: wgpu::Queue,
  surface_config: wgpu::SurfaceConfiguration,
  surface: wgpu::Surface<'static>,
  egui_renderer: EguiRenderer,
  scene_renderer: SceneRenderer,
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

    let scene_renderer = SceneRenderer::new(&device, surface_config.format);

    Self {
      device,
      queue,
      surface_config,
      surface,
      egui_renderer,
      scene_renderer,
    }
  }

  fn resize_surface(&mut self, new_width: u32, new_height: u32) {
    self.surface_config.width = new_width;
    self.surface_config.height = new_height;
    self.surface.configure(&self.device, &self.surface_config);
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
    let initial_width = 1360;
    let initial_height = 768;

    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(initial_width, initial_height));

    let surface = self
      .instance
      .create_surface(window.clone())
      .expect("Failed to create surface!");

    let state = AppState::new(
      &self.instance,
      surface,
      &window,
      initial_width,
      initial_width,
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

    state.scene_renderer.render(&mut encoder, &surface_view);

    let window = self.window.get().expect("Window not initialized!");

    {
      state.egui_renderer.begin_frame(window);

      egui::Window::new("winit + egui + wgpu says hello!")
        .resizable(true)
        .vscroll(true)
        .default_open(false)
        .show(state.egui_renderer.context(), |ui| {
          ui.label("Label");

          if ui.button("Button").clicked() {
            println!("clicked!")
          }

          ui.separator();
          ui.horizontal(|ui| {
            ui.label(format!(
              "Pixels per point: {}",
              state.egui_renderer.context().pixels_per_point()
            ));
          });
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
      .create_window(winit::window::Window::default_attributes())
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
      _ => (),
    }
  }
}
