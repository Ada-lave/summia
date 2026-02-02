#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod mlx;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod llama_cpp;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SummaryError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Server unavailable: {0}")]
    ServerUnavailable(String),
}

/// Трейт для суммаризации текста
pub trait Summarizer: Send + Sync {
    /// Суммаризирует текст и возвращает краткое содержание
    fn summarize(&self, text: &str) -> Result<String, SummaryError>;
}

/// Создаёт подходящий Summarizer в зависимости от платформы:
/// - macOS Apple Silicon → MLX (HTTP к локальному серверу)
/// - Остальные → llama.cpp (нативный инференс)
pub fn create_summarizer() -> Result<Box<dyn Summarizer>, SummaryError> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(Box::new(mlx::MlxSummarizer::new()?))
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        Ok(Box::new(llama_cpp::LlamaCppSummarizer::new()?))
    }
}
