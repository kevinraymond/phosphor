use wgpu::{Device, Sampler, Texture, TextureFormat, TextureView};

/// An off-screen render target with texture, view, and sampler.
pub struct RenderTarget {
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl RenderTarget {
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        format: TextureFormat,
        scale: f32,
        label: &str,
    ) -> Self {
        let w = ((width as f32 * scale) as u32).max(1);
        let h = ((height as f32 * scale) as u32).max(1);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{label}-sampler")),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            format,
            width: w,
            height: h,
            scale,
        }
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        let w = ((width as f32 * self.scale) as u32).max(1);
        let h = ((height as f32 * self.scale) as u32).max(1);
        if w == self.width && h == self.height {
            return;
        }
        *self = Self::new(device, width, height, self.format, self.scale, "render-target");
    }
}

/// Two render targets for ping-pong feedback (previous frame access).
pub struct PingPongTarget {
    pub targets: [RenderTarget; 2],
    pub current: usize,
}

impl PingPongTarget {
    pub fn new(
        device: &Device,
        width: u32,
        height: u32,
        format: TextureFormat,
        scale: f32,
    ) -> Self {
        let a = RenderTarget::new(device, width, height, format, scale, "feedback-a");
        let b = RenderTarget::new(device, width, height, format, scale, "feedback-b");
        Self {
            targets: [a, b],
            current: 0,
        }
    }

    /// The target we render the current frame into.
    pub fn write_target(&self) -> &RenderTarget {
        &self.targets[self.current]
    }

    /// The target containing the previous frame's output.
    pub fn read_target(&self) -> &RenderTarget {
        &self.targets[1 - self.current]
    }

    pub fn flip(&mut self) {
        self.current = 1 - self.current;
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        self.targets[0].resize(device, width, height);
        self.targets[1].resize(device, width, height);
    }
}
