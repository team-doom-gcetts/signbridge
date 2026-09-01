
use log::*;
use wgpu::*;
use flume::Sender;
use std::sync::Arc;
use crate::RgbImageBuffer;


use nokhwa::{
  Camera,
  pixel_format::{
    RgbFormat,
    RgbAFormat,
  },
  utils::{CameraIndex, RequestedFormat, RequestedFormatType},
};

use winit::{
  application::ApplicationHandler,
  dpi::LogicalSize,
  event::WindowEvent,
  event_loop::ActiveEventLoop,
  window::{Window, WindowId},
};

const SHADER: &str=include_str!("./shader.wgsl");
const THRESHOLD_FRAME_COUNT: u64=15;


struct State {
  window: Arc<Window>,
  surface: Surface<'static>,
  device: Device,
  queue: Queue,
  config: SurfaceConfiguration,
  pipeline: RenderPipeline,
  sampler: Sampler,
  camera: Camera,
  tx: Sender<Arc<RgbImageBuffer>>,
  frame_index: u64,
  camera_bind_group_layout: BindGroupLayout,
}

impl State {
  async fn new(window: Window,tx: Sender<Arc<RgbImageBuffer>>)-> Self {
    let window=Arc::new(window);
    let instance=Instance::default();

    let surface=instance
    .create_surface(window.clone())
    .expect("failed to create surface");

    let adapter=instance
    .request_adapter(&RequestAdapterOptions {
      power_preference: PowerPreference::HighPerformance,
      compatible_surface: Some(&surface),
      force_fallback_adapter: false,
    })
    .await
    .expect("failed to find GPU adapter");

    info!("GPU: {}", adapter.get_info().name);

    let (device, queue)=adapter
    .request_device(&DeviceDescriptor {
      experimental_features: ExperimentalFeatures::disabled(),
      label: Some("device"),
      required_features: Features::empty(),
      required_limits: Limits::default(),
      memory_hints: MemoryHints::Performance,
      trace: Trace::Off,
    })
    .await
    .expect("failed to create device");

    let size=window.inner_size();

    let capabilities=surface.get_capabilities(&adapter);

    let surface_format=capabilities.formats.iter()
    .copied()
    .find(|f| f.is_srgb())
    .unwrap_or(capabilities.formats[0]);

    let config=SurfaceConfiguration {
      usage: TextureUsages::RENDER_ATTACHMENT,
      format: surface_format,
      width: size.width.max(1),
      height: size.height.max(1),
      present_mode: PresentMode::Fifo,
      alpha_mode: capabilities.alpha_modes[0],
      view_formats: vec![],
      desired_maximum_frame_latency: 2,
    };

    surface.configure(&device, &config);

    // ------------------------------------------------------------
    // Camera
    // ------------------------------------------------------------

    let requested=RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);

    let mut camera=Camera::new(CameraIndex::Index(0), requested).expect("failed to create camera");

    camera.open_stream().expect("failed to open camera stream");

    info!("Camera resolution: {:?}", camera.resolution());
    info!("Camera FPS: {:?}", camera.frame_rate());

    // ------------------------------------------------------------
    // Shader
    // ------------------------------------------------------------

    let shader=device.create_shader_module(ShaderModuleDescriptor {
      label: Some("camera shader"),
      source: ShaderSource::Wgsl(SHADER.into()),
    });

    // ------------------------------------------------------------
    // Camera texture bind group layout
    // ------------------------------------------------------------

    let camera_bind_group_layout=device.create_bind_group_layout(&BindGroupLayoutDescriptor {
      label: Some("camera bind group layout"),
        entries: &[
        // Texture
        BindGroupLayoutEntry {
          binding: 0,
          visibility: ShaderStages::FRAGMENT,
          ty: BindingType::Texture {
            multisampled: false,
            view_dimension: TextureViewDimension::D2,
            sample_type: TextureSampleType::Float { filterable: true },
          },
          count: None,
        },
        // Sampler
        BindGroupLayoutEntry {
          binding: 1,
          visibility: ShaderStages::FRAGMENT,
          ty: BindingType::Sampler(SamplerBindingType::Filtering),
          count: None,
        },
      ],
    });

    // ------------------------------------------------------------
    // Sampler
    // ------------------------------------------------------------

    let sampler=device.create_sampler(&SamplerDescriptor {
      label: Some("camera sampler"),
      address_mode_u: AddressMode::ClampToEdge,
      address_mode_v: AddressMode::ClampToEdge,
      address_mode_w: AddressMode::ClampToEdge,
      mag_filter: FilterMode::Linear,
      min_filter: FilterMode::Linear,
      mipmap_filter: FilterMode::Nearest,
      ..Default::default()
    });

    // ------------------------------------------------------------
    // Pipeline
    // ------------------------------------------------------------

    let pipeline_layout=device.create_pipeline_layout(&PipelineLayoutDescriptor {
      label: Some("camera pipeline layout"),
      bind_group_layouts: &[&camera_bind_group_layout],
      push_constant_ranges: &[],
    });

    let pipeline=device.create_render_pipeline(&RenderPipelineDescriptor {
      label: Some("camera pipeline"),
      layout: Some(&pipeline_layout),
      vertex: VertexState {
        module: &shader,
        entry_point: Some("vs_main"),
        buffers: &[],
        compilation_options: Default::default(),
      },
      fragment: Some(FragmentState {
        module: &shader,
        entry_point: Some("fs_main"),
        targets: &[Some(ColorTargetState {
          format: surface_format,
          blend: Some(BlendState::REPLACE),
          write_mask: ColorWrites::ALL,
        })],
        compilation_options: Default::default(),
      }),
      primitive: PrimitiveState {
        topology: PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: FrontFace::Ccw,
        cull_mode: None,
        polygon_mode: PolygonMode::Fill,
        unclipped_depth: false,
        conservative: false,
      },
      depth_stencil: None,
      multisample: MultisampleState::default(),
      multiview: None,
      cache: None,
    });

    Self {
      tx,
      window,
      surface,
      device,
      queue,
      config,
      pipeline,
      sampler,
      camera,
      frame_index: 0,
      camera_bind_group_layout,
    }
  }

  fn resize(&mut self, width: u32, height: u32) {
    if width==0 || height==0 {
      return;
    }

    self.config.width=width;
    self.config.height=height;

    self.surface.configure(&self.device, &self.config);
  }

  fn frame_texture(&mut self)-> anyhow::Result<Texture> {
    let frame=self.camera.frame()?
    .decode_image::<RgbAFormat>()?;
    let frame=Arc::new(frame);

    if self.frame_index%THRESHOLD_FRAME_COUNT==0 {
      self.tx.send(frame.clone())?;
    }
    self.frame_index=self.frame_index.wrapping_add(1);

    let texture_size=Extent3d {
      width: frame.width(),
      height: frame.height(),
      depth_or_array_layers: 1,
    };

    let texture=self.device.create_texture(&TextureDescriptor {
      label: Some("camera frame"),
      size: texture_size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: TextureDimension::D2,
      format: TextureFormat::Rgba8UnormSrgb,
      usage: TextureUsages::TEXTURE_BINDING|TextureUsages::COPY_DST,
      view_formats: &[],
    });

    let width_nonzero=4*frame.width();
    let height_nonzero=frame.height();

    self.queue.write_texture(
      TexelCopyTextureInfoBase {
        texture: &texture,
        mip_level: 0,
        origin: Origin3d { x: 0, y: 0, z: 0 },
        aspect: TextureAspect::All,
      },
      &frame,
      TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(width_nonzero),
        rows_per_image: Some(height_nonzero),
      },
      texture_size,
    );

    Ok(texture)
  }

  fn render(&mut self) {
    // ------------------------------------------------------------
    // Get camera frame directly as WGPU texture
    // ------------------------------------------------------------

    let camera_texture=match self.frame_texture() {
      Ok(texture)=> texture,
      Err(error)=> {
        error!("camera error: {error:?}");
        return;
      }
    };

    // ------------------------------------------------------------
    // Create texture view
    // ------------------------------------------------------------

    let camera_view=camera_texture.create_view(&TextureViewDescriptor::default());

    // ------------------------------------------------------------
    // Bind camera texture
    // ------------------------------------------------------------

    let bind_group=self.device.create_bind_group(&BindGroupDescriptor {
      label: Some("camera bind group"),
      layout: &self.camera_bind_group_layout,
      entries: &[
        BindGroupEntry {
          binding: 0,
          resource: BindingResource::TextureView(&camera_view),
        },
        BindGroupEntry {
          binding: 1,
          resource: BindingResource::Sampler(&self.sampler),
        },
      ],
    });

    // ------------------------------------------------------------
    // Surface
    // ------------------------------------------------------------

    let output=match self.surface.get_current_texture() {
      Ok(output)=> output,
      Err(SurfaceError::Lost)=> {
        self.surface.configure(&self.device, &self.config);
        return;
      },
      Err(SurfaceError::OutOfMemory)=> {
        panic!("GPU out of memory");
      },
      Err(error)=> {
        error!("surface error: {error:?}");
        return;
      }
    };

    let view=output.texture
    .create_view(&TextureViewDescriptor::default());

    // ------------------------------------------------------------
    // Render pass
    // ------------------------------------------------------------

    let mut encoder=self.device
    .create_command_encoder(&CommandEncoderDescriptor {
      label: Some("render encoder"),
    });

    {
      let mut render_pass=encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("camera render pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
          view: &view,
          depth_slice: None,
          resolve_target: None,
          ops: Operations {
            load: LoadOp::Clear(Color::BLACK),
            store: StoreOp::Store,
          },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
      });

      render_pass.set_pipeline(&self.pipeline);

      render_pass.set_bind_group(0, &bind_group, &[]);

      render_pass.draw(0..6, 0..1);
    }

    // ------------------------------------------------------------
    // Submit
    // ------------------------------------------------------------

    self.queue.submit(Some(encoder.finish()));

    output.present();
  }
}

pub struct App {
  tx: Sender<Arc<RgbImageBuffer>>,
  state: Option<State>,
}

impl App {
  pub fn new(tx: Sender<Arc<RgbImageBuffer>>)-> Self {
    Self {
      tx,
      state: None,
    }
  }
}

impl ApplicationHandler for App {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.state.is_some() {
      return;
    }

    let attributes=Window::default_attributes()
    .with_title("signbridge")
    .with_inner_size(LogicalSize::new(1280, 720));

    let window=event_loop
    .create_window(attributes)
    .expect("failed to create window");

    self.state=Some(pollster::block_on(State::new(window,self.tx.clone())));

    if let Some(state)=&self.state {
      state.window.request_redraw();
    }
  }

  fn window_event(&mut self,event_loop: &ActiveEventLoop,_window_id: WindowId,event: WindowEvent) {
    let Some(state)=&mut self.state else {
      return;
    };

    match event {
      WindowEvent::CloseRequested=> {
        event_loop.exit();
      },
      WindowEvent::Resized(size)=> {
        state.resize(size.width, size.height);
      },
      WindowEvent::RedrawRequested=> {
        state.render();

        // Keep rendering continuously.
        state.window.request_redraw();
      },
      _=> {},
    }
  }
}
