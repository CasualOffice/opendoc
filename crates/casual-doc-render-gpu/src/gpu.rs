//! The GPU painter: `vello_hybrid` on `wgpu`.
//!
//! `vello_hybrid` splits rasterization: the CPU flattens paths into sparse
//! strips (the coverage of the pixels an edge crosses), and the GPU fills them
//! and composites. That split is why it runs on WebGL2-class hardware as well as
//! WebGPU, and why the CPU still pays for path flattening (`docs/architecture/179` §4).
//!
//! [`GpuRenderer`] owns one device, one queue and one renderer, and keeps the
//! render target and the readback buffer between pages of the same size, so a
//! second page costs the encode and the GPU work, not the setup.
//!
//! [`GpuRenderer::render_rgba`] reads the page back as premultiplied RGBA, the
//! exact contract `casual_doc_render::Surface::data` has, which is what the
//! parity test and an RGBA host (a canvas `putImageData`) need. A host that owns
//! a surface (a WebGPU canvas) renders into its own texture with
//! [`GpuRenderer::render_to_view`] and skips the readback entirely.

use casual_doc_layout::display::DisplayList;
use casual_doc_render::{GlyphSource, MediaSource};
use vello_common::kurbo::{Affine, BezPath, Rect as KRect, Stroke};
use vello_common::paint::{ImageSource, PaintType};
use vello_common::peniko::{BlendMode, Color};
use vello_common::pixmap::Pixmap;
use vello_hybrid::{RenderSize, RenderTargetConfig, Renderer, Resources, Scene, TextureBindings};

use crate::encode::{EncodeStats, SparseCanvas, encode_display_list};

/// Why the GPU backend could not render. Every variant means "use the CPU
/// raster for this page", which is always available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// No adapter: no GPU, no driver, or (in a browser) no WebGPU.
    NoAdapter(String),
    /// The adapter refused a device.
    NoDevice(String),
    /// The page is larger than the device's largest texture, or than the
    /// renderer's 16-bit scene coordinates (65,535 px).
    TooLarge {
        /// Requested width in device pixels.
        width: u32,
        /// Requested height in device pixels.
        height: u32,
        /// The largest side the device accepts.
        max: u32,
    },
    /// The renderer reported an error.
    Render(String),
    /// Reading the pixels back failed.
    Readback(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter(why) => write!(f, "no GPU adapter: {why}"),
            Self::NoDevice(why) => write!(f, "no GPU device: {why}"),
            Self::TooLarge { width, height, max } => {
                write!(f, "{width}x{height} exceeds the GPU's {max} px limit")
            }
            Self::Render(why) => write!(f, "GPU render failed: {why}"),
            Self::Readback(why) => write!(f, "GPU readback failed: {why}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// A page rendered on the GPU and read back.
#[derive(Debug, Clone)]
pub struct GpuPage {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Premultiplied RGBA, row-major, no padding — `Surface::data`'s layout.
    pub rgba: Vec<u8>,
    /// What the encode did (how much went through the CPU fallback).
    pub stats: EncodeStats,
}

/// The texture format of the render target.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Target {
    width: u32,
    height: u32,
    renderer: Renderer,
    resources: Resources,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row: u32,
}

/// One GPU device rendering display lists. See the module documentation.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: String,
    scene: Scene,
    target: Option<Target>,
}

impl std::fmt::Debug for GpuRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuRenderer")
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}

impl GpuRenderer {
    /// Requests a headless adapter and device. Fails, rather than panicking,
    /// when the machine has no usable GPU, so the caller keeps the CPU raster.
    pub async fn new() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("opendoc-render-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::NoDevice(e.to_string()))?;
        Ok(Self::from_device(
            device,
            queue,
            format!("{} ({:?})", info.name, info.backend),
        ))
    }

    /// [`GpuRenderer::new`], blocking. Native only: a browser cannot block.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_blocking() -> Result<Self, GpuError> {
        pollster::block_on(Self::new())
    }

    /// Wraps a device the host already owns (a WebGPU canvas's, for one).
    #[must_use]
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue, adapter: String) -> Self {
        Self {
            device,
            queue,
            adapter,
            scene: Scene::new(1, 1),
            target: None,
        }
    }

    /// The adapter's name and backend, for a diagnostic line.
    #[must_use]
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The largest page side, in device pixels, this device can render.
    #[must_use]
    pub fn max_dimension(&self) -> u32 {
        self.device
            .limits()
            .max_texture_dimension_2d
            .min(u32::from(u16::MAX))
    }

    fn check_size(&self, width: u32, height: u32) -> Result<(), GpuError> {
        let max = self.max_dimension();
        if width == 0 || height == 0 || width > max || height > max {
            return Err(GpuError::TooLarge { width, height, max });
        }
        Ok(())
    }

    fn ensure_target(&mut self, width: u32, height: u32) {
        if self
            .target
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height)
        {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("opendoc page"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opendoc page readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let (renderer, resources) = Renderer::new(
            &self.device,
            &RenderTargetConfig {
                format: FORMAT,
                width,
                height,
            },
        );
        self.target = Some(Target {
            width,
            height,
            renderer,
            resources,
            texture,
            view,
            readback,
            padded_row,
        });
    }

    /// Encodes `list` into the scene and submits it into `view`, cleared to
    /// `background`. Returns the encode statistics; the caller decides whether
    /// to read back.
    #[allow(clippy::too_many_arguments)] // the CPU `render` inputs plus the target
    fn encode_and_submit(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        dpi: f32,
        background: [u8; 3],
        fonts: &dyn GlyphSource,
        media: &dyn MediaSource,
        external_view: Option<&wgpu::TextureView>,
        readback: bool,
    ) -> Result<EncodeStats, GpuError> {
        self.check_size(width, height)?;
        self.ensure_target(width, height);
        let target = self.target.as_mut().expect("ensured");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("opendoc page"),
            });
        self.scene.reset_and_resize(width as u16, height as u16);
        let [r, g, b] = background;
        self.scene.set_paint(Color::from_rgba8(r, g, b, 255));
        self.scene
            .fill_rect(&KRect::new(0.0, 0.0, f64::from(width), f64::from(height)));
        let mut uploaded = Vec::new();
        let stats = {
            let mut canvas = HybridCanvas {
                scene: &mut self.scene,
                renderer: &mut target.renderer,
                resources: &mut target.resources,
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                uploaded: &mut uploaded,
            };
            encode_display_list(list, &mut canvas, width, height, dpi, fonts, media)
        };
        let view = external_view.unwrap_or(&target.view);
        target
            .renderer
            .render(
                &self.scene,
                &mut target.resources,
                &self.device,
                &self.queue,
                &mut encoder,
                &RenderSize { width, height },
                view,
                &TextureBindings::new(),
            )
            .map_err(|e| GpuError::Render(e.to_string()))?;
        // Fallback tiles live for one page: release their atlas space so a
        // long session does not grow the atlas page by page.
        for id in uploaded {
            target
                .renderer
                .destroy_image(&mut target.resources, &mut encoder, id);
        }
        if readback {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &target.readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(target.padded_row),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.submit([encoder.finish()]);
        Ok(stats)
    }

    /// Renders a page into a texture view the host owns (a WebGPU canvas's
    /// current texture), in [`wgpu::TextureFormat::Rgba8Unorm`]. No readback:
    /// the pixels never leave the GPU.
    #[allow(clippy::too_many_arguments)] // the CPU `render` inputs plus the target
    pub fn render_to_view(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        dpi: f32,
        background: [u8; 3],
        fonts: &dyn GlyphSource,
        media: &dyn MediaSource,
        view: &wgpu::TextureView,
    ) -> Result<EncodeStats, GpuError> {
        self.encode_and_submit(
            list,
            width,
            height,
            dpi,
            background,
            fonts,
            media,
            Some(view),
            false,
        )
    }

    /// Renders a page and reads it back as premultiplied RGBA. Native only (a
    /// browser cannot block on the map); a browser host renders to its canvas
    /// with [`GpuRenderer::render_to_view`].
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)] // the CPU `render` inputs
    pub fn render_rgba(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        dpi: f32,
        background: [u8; 3],
        fonts: &dyn GlyphSource,
        media: &dyn MediaSource,
    ) -> Result<GpuPage, GpuError> {
        let stats = self.encode_and_submit(
            list, width, height, dpi, background, fonts, media, None, true,
        )?;
        let target = self.target.as_ref().expect("ensured");
        let slice = target.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        receiver
            .recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        let row = (width * 4) as usize;
        let mut rgba = Vec::with_capacity(row * height as usize);
        {
            let mapped = slice.get_mapped_range();
            for line in mapped.chunks_exact(target.padded_row as usize) {
                rgba.extend_from_slice(&line[..row]);
            }
        }
        target.readback.unmap();
        Ok(GpuPage {
            width,
            height,
            rgba,
            stats,
        })
    }

    /// Blocks until every submitted page has finished on the GPU. For a
    /// benchmark that times [`GpuRenderer::render_to_view`] honestly.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_idle(&self) -> Result<(), GpuError> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|e| GpuError::Render(e.to_string()))
    }

    /// A texture of the page size the benchmark can render into without a
    /// readback, standing in for a canvas's surface texture.
    #[must_use]
    pub fn offscreen_view(&self, width: u32, height: u32) -> wgpu::TextureView {
        self.device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("opendoc offscreen"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}

struct HybridCanvas<'a> {
    scene: &'a mut Scene,
    renderer: &'a mut Renderer,
    resources: &'a mut Resources,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    encoder: &'a mut wgpu::CommandEncoder,
    uploaded: &'a mut Vec<vello_common::paint::ImageId>,
}

impl SparseCanvas for HybridCanvas<'_> {
    fn set_transform(&mut self, transform: Affine) {
        self.scene.set_transform(transform);
    }
    fn set_paint(&mut self, paint: PaintType) {
        self.scene.set_paint(paint);
    }
    fn set_paint_transform(&mut self, transform: Affine) {
        self.scene.set_paint_transform(transform);
    }
    fn reset_paint_transform(&mut self) {
        self.scene.reset_paint_transform();
    }
    fn set_stroke(&mut self, stroke: Stroke) {
        self.scene.set_stroke(stroke);
    }
    fn set_fill_rule(&mut self, rule: vello_common::peniko::Fill) {
        self.scene.set_fill_rule(rule);
    }
    fn fill_path(&mut self, path: &BezPath) {
        self.scene.fill_path(path);
    }
    fn stroke_path(&mut self, path: &BezPath) {
        self.scene.stroke_path(path);
    }
    fn fill_rect(&mut self, rect: &KRect) {
        self.scene.fill_rect(rect);
    }
    fn push_clip_layer(&mut self, path: &BezPath) {
        self.scene.push_clip_layer(path);
    }
    fn push_blend_layer(&mut self, blend: BlendMode) {
        self.scene.push_blend_layer(blend);
    }
    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }
    fn register_image(&mut self, pixmap: Pixmap) -> ImageSource {
        let id = self.renderer.upload_image(
            self.resources,
            self.device,
            self.queue,
            self.encoder,
            &pixmap,
        );
        self.uploaded.push(id);
        ImageSource::opaque_id(id)
    }
}
