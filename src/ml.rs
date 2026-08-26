
use std::sync::Arc;
use flume::Receiver;
use image::{
  Rgba,
  ImageBuffer,
};


pub type RgbImageBuffer=ImageBuffer<Rgba<u8>,Vec<u8>>;


pub fn ml_main(rx: Receiver<Arc<RgbImageBuffer>>)-> anyhow::Result<()> {

  while let Ok(frame)=rx.recv() {
  }


  Ok(())
}








