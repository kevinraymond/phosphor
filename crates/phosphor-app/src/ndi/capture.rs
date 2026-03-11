use wgpu::{CommandEncoder, Device, TextureFormat, TextureView};

use crate::gpu::frame_capture::FrameCapture;

/// GPU capture target for NDI output. Thin wrapper around shared `FrameCapture`.
pub struct NdiCapture {
    inner: FrameCapture,
}

impl NdiCapture {
    pub fn new(device: &Device, width: u32, height: u32, format: TextureFormat) -> Self {
        Self {
            inner: FrameCapture::new(device, width, height, format, "ndi-capture"),
        }
    }

    pub fn view(&self) -> &TextureView {
        &self.inner.view
    }

    pub fn width(&self) -> u32 {
        self.inner.width
    }

    pub fn height(&self) -> u32 {
        self.inner.height
    }

    pub fn copy_to_staging(&self, encoder: &mut CommandEncoder) {
        self.inner.copy_to_staging(encoder);
    }

    pub fn is_map_pending(&self) -> bool {
        self.inner.is_map_pending()
    }

    pub fn request_map(&mut self) {
        self.inner.request_map();
    }

    pub fn take_mapped_data(&mut self, device: &Device) -> Option<Vec<u8>> {
        self.inner.take_mapped_data(device)
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        self.inner.resize(device, width, height, "ndi-capture");
    }
}
