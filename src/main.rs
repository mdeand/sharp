pub mod app;
pub mod gfx;
pub mod scenario;

async fn run() {
  let event_loop = winit::event_loop::EventLoop::new().unwrap();

  event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

  let mut app = app::App::new();

  event_loop.run_app(&mut app).expect("Failed to run app");
}

fn main() {
  pollster::block_on(run());
}
