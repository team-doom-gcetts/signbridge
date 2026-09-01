
mod ml;
mod app;


use app::App;
use flume::Sender;
use std::sync::Arc;
use ml::RgbImageBuffer;

use winit::event_loop::{
  EventLoop,
  ControlFlow,
};


fn main()-> anyhow::Result<()> {
  env_logger::init();

  let (tx,rx)=flume::unbounded::<Arc<RgbImageBuffer>>();

  rayon::spawn(|| ml::ml_main(rx).unwrap());

  app(tx);
  Ok(())
}







fn app(tx: Sender<Arc<RgbImageBuffer>>) {
  let event_loop=EventLoop::new().unwrap();

  event_loop.set_control_flow(ControlFlow::Poll);

  let mut app=App::new(tx);
  event_loop.run_app(&mut app)
  .unwrap();
}



