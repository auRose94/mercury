//! Render backends — gold's `gfxBackend` seam: the engine calls `pre_frame`
//! before the update pass and `render_frame` after the draw pass. The wgpu
//! backend (behind the `wgpu` feature) renders offscreen — no window or
//! surface needed — so frames can be pixel-tested and screenshotted
//! headlessly, exactly like gold's `{"screenshot", path}` debug aid.

/// A rendering backend (gold's `renderBackend`), as a trait.
pub trait RenderBackend: Send {
    fn name(&self) -> &str;

    /// Per-frame setup, before the update pass.
    fn pre_frame(&mut self) {}

    /// Draw the frame. Called after the engine's draw pass.
    fn render_frame(&mut self) {}
}
