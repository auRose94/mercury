//! Render backends — gold's `gfxBackend` seam: the engine calls `pre_frame`
//! before the update pass and `render_frame` after the draw pass. The wgpu
//! backend (behind the `wgpu` feature) renders offscreen — no window or
//! surface needed — so frames can be pixel-tested and screenshotted
//! headlessly, exactly like gold's `{"screenshot", path}` debug aid.

/// One solid quad in clip space: center `pos`, half-extents `size`, `color`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

/// One rendered frame, RGBA8, top to bottom.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Monotonic counter — consumers skip frames they've already seen.
    pub generation: u64,
    pub rgba: Vec<u8>,
}

/// A shared slot for the latest rendered frame. The renderer publishes
/// into it; anything else (a streaming window backend, a debug tool) reads
/// the newest frame without coupling to the renderer.
#[derive(Clone, Default)]
pub struct FrameSink(std::sync::Arc<std::sync::Mutex<Inner>>);

#[derive(Default)]
struct Inner {
    frame: Option<Frame>,
    generation: u64,
}

impl FrameSink {
    pub fn new() -> Self { Self::default() }

    /// Publish a frame (bumps the generation).
    pub fn publish(&self, width: u32, height: u32, rgba: Vec<u8>) {
        let mut inner = self.0.lock().unwrap();
        inner.generation += 1;
        inner.frame = Some(Frame {
            width,
            height,
            generation: inner.generation,
            rgba,
        });
    }

    /// The newest frame, if any.
    pub fn latest(&self) -> Option<Frame> { self.0.lock().unwrap().frame.clone() }

    /// The newest frame whose generation is greater than `seen`.
    pub fn latest_since(&self, seen: u64) -> Option<Frame> {
        let inner = self.0.lock().unwrap();
        match &inner.frame {
            Some(f) if f.generation > seen => Some(f.clone()),
            _ => None,
        }
    }
}

/// A rendering backend (gold's `renderBackend`), as a trait.
pub trait RenderBackend: Send {
    fn name(&self) -> &str;

    /// Per-frame setup, before the update pass.
    fn pre_frame(&mut self) {}

    /// Draw the frame. Called after the engine's draw pass.
    fn render_frame(&mut self) {}
}
