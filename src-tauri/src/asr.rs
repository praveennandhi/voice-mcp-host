use anyhow::{Context, Result};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
}

// WhisperContext is internally thread-safe
unsafe impl Send for Transcriber {}
unsafe impl Sync for Transcriber {}

impl Transcriber {
    pub fn load(model_path: &Path) -> Result<Self> {
        let path_str = model_path
            .to_str()
            .context("model path is not valid UTF-8")?;
        let ctx = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .context("failed to load Whisper model")?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio: &[f32], language: &str) -> Result<String> {
        let mut state = self.ctx.create_state().context("failed to create Whisper state")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        state.full(params, audio).context("Whisper inference failed")?;

        let n = state.full_n_segments().context("failed to get segment count")?;
        let mut text = String::new();
        for i in 0..n {
            let seg = state.full_get_segment_text(i).context("failed to get segment text")?;
            let trimmed = seg.trim();
            if !trimmed.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(trimmed);
            }
        }

        Ok(text)
    }
}
