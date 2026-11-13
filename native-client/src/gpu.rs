//! The GPU path (cargo feature "gpu", turned on with CRUNDI_GPU=1).
//!
//! CEF renders on the GPU and hands each frame over as a shared texture
//! (on_accelerated_paint): a DMA-BUF on Linux, a D3D11 shared handle on
//! Windows, an IOSurface on macOS. cef-rs copies it into a wgpu texture on
//! Slint's own wgpu device, and one render pass turns CEF's BGRA into the RGBA
//! Slint takes, so the page never passes through the CPU.
//!
//! Slint must render with wgpu for this (its femtovg-wgpu renderer), on the
//! backend each platform's import needs: Vulkan with DMA-BUF on Linux, D3D12 on
//! Windows, Metal on macOS.

use cef::{osr_texture_import::SharedTextureHandle, AcceleratedPaintInfo};
use slint::wgpu_30::{wgpu, WGPUConfiguration, WGPUSettings};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};

static ON: AtomicBool = AtomicBool::new(false);

/// Is the GPU path in use (asked for, and Slint is rendering with wgpu)?
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Before any window: make Slint render with wgpu on the backend CEF's
/// textures can be imported into. False (and the CPU path) when it can't.
pub fn select_backend() -> bool {
    if std::env::var("CRUNDI_GPU").map(|v| v != "1").unwrap_or(true) {
        return false;
    }
    let mut s = WGPUSettings::default();
    #[cfg(target_os = "linux")]
    {
        s.backends = wgpu::Backends::VULKAN;
        s.device_required_features = wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF;
    }
    #[cfg(target_os = "windows")]
    {
        s.backends = wgpu::Backends::DX12;
    }
    #[cfg(target_os = "macos")]
    {
        s.backends = wgpu::Backends::METAL;
    }
    match slint::BackendSelector::new().backend_name("winit".into()).require_wgpu_30(WGPUConfiguration::Automatic(s)).select() {
        Ok(()) => {
            ON.store(true, Ordering::Relaxed);
            eprintln!("[native] GPU path: Slint renders with wgpu, CEF hands over shared textures");
            true
        }
        Err(e) => {
            eprintln!("[native] GPU path unavailable ({e}); using the CPU path");
            false
        }
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

thread_local! {
    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
}

/// Take Slint's device and queue once its renderer is set up, then call
/// [ready] (the panels repaint: frames that came before had nowhere to go).
pub fn attach(window: &slint::Window, ready: impl Fn() + 'static) {
    let r = window.set_rendering_notifier(move |state, api| {
        if let (slint::RenderingState::RenderingSetup, slint::GraphicsAPI::WGPU30 { device, queue, .. }) = (state, api) {
            GPU.with(|g| *g.borrow_mut() = Some(Gpu::new(device.clone(), queue.clone())));
            eprintln!("[native] GPU path: Slint's wgpu device is ready");
            ready();
        }
    });
    if let Err(e) = r {
        eprintln!("[native] GPU path: no rendering notifier ({e:?})");
    }
}

const SHADER: &str = r#"
@group(0) @binding(0) var page: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    // One triangle that covers the target.
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: V;
    o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}
// Sampling reads the BGRA texture as RGBA values; writing them to an RGBA
// target is the whole conversion.
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return textureSample(page, smp, v.uv); }
"#;

impl Gpu {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("bgra-to-rgba"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("page"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("page"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bgra-to-rgba"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Gpu { device, queue, pipeline, layout, sampler }
    }

    /// CEF's frame (BGRA or RGBA) into a new RGBA texture Slint can show.
    fn to_rgba(&self, src: &wgpu::Texture) -> wgpu::Texture {
        let dst = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("page frame"),
            size: src.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let src_view = src.create_view(&Default::default());
        let dst_view = dst.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("page"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("page frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bgra-to-rgba"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
        dst
    }
}

/// Inside on_accelerated_paint (the shared texture is only valid there): the
/// frame as a Slint image, or None when the device isn't ready or the import fails.
pub fn import(info: &AcceleratedPaintInfo) -> Option<slint::Image> {
    GPU.with(|g| {
        let g = g.borrow();
        let g = g.as_ref()?;
        // SAFETY: called inside CEF's paint callback with its current frame, on
        // the device Slint renders with (the same GPU as CEF's).
        let tex = match unsafe { SharedTextureHandle::new(info).import_texture(&g.device, &g.queue) } {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[native] GPU path: import failed: {e}");
                return None;
            }
        };
        let rgba = if tex.format() == wgpu::TextureFormat::Rgba8Unorm { tex } else { g.to_rgba(&tex) };
        slint::Image::try_from(rgba).map_err(|e| eprintln!("[native] GPU path: Slint refused the texture: {e:?}")).ok()
    })
}
