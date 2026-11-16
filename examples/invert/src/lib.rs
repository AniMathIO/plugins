//! Straight-alpha sRGB inversion; applying twice restores the original bytes.
wit_bindgen::generate!({ path: "wit", world: "effect-plugin" });
use animathio::plugin::types::*;
use exports::animathio::plugin::frame_effects::{EffectDescriptor, Guest as FrameEffects};
struct Invert;
impl Guest for Invert {
    fn describe() -> Manifest {
        Manifest {
            name: "Invert".into(),
            version: "1.0.0".into(),
            description: "Invert RGB while preserving alpha".into(),
            kinds: vec!["effect".into()],
            metadata: vec![],
            params: vec![],
        }
    }
}
impl FrameEffects for Invert {
    fn list_effects() -> Vec<EffectDescriptor> {
        vec![EffectDescriptor {
            id: "invert".into(),
            name: "Invert".into(),
            description: "Invert RGB while preserving alpha".into(),
            params: vec![],
        }]
    }
    fn apply_frame(
        effect: String,
        mut pixels: Vec<u8>,
        size: Size,
        _params: Vec<ParamEntry>,
        _time_ticks: i64,
    ) -> Result<Vec<u8>, Diagnostic> {
        let expected = (size.width as usize)
            .checked_mul(size.height as usize)
            .and_then(|n| n.checked_mul(4));
        if effect != "invert"
            || size.width == 0
            || size.height == 0
            || expected != Some(pixels.len())
        {
            return Err(Diagnostic {
                code: "invalid-input".into(),
                message: "Unknown effect or invalid RGBA dimensions".into(),
                metadata: vec![],
            });
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                *channel = 255 - *channel;
            }
        }
        Ok(pixels)
    }
}
export!(Invert);
