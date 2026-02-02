use super::{SummaryError, Summarizer};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use std::path::Path;

const MODEL_PATH: &str = "models/phi-3-mini-4k-instruct-q4.gguf";
const CONTEXT_SIZE: u32 = 2048;
const MAX_TOKENS: usize = 1024;

pub struct LlamaCppSummarizer {
    backend: LlamaBackend,
    model_path: String,
}

impl LlamaCppSummarizer {
    pub fn new() -> Result<Self, SummaryError> {
        let backend = LlamaBackend::init()
            .map_err(|e| SummaryError::InferenceFailed(format!("Failed to init backend: {}", e)))?;

        // Проверяем наличие модели
        if !Path::new(MODEL_PATH).exists() {
            return Err(SummaryError::ModelNotFound(format!(
                "Model not found at '{}'. Download from HuggingFace:\n\
                wget https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf -O {}",
                MODEL_PATH, MODEL_PATH
            )));
        }

        Ok(Self {
            backend,
            model_path: MODEL_PATH.into(),
        })
    }

    /// Создаёт LlamaCppSummarizer с кастомным путём к модели
    #[allow(dead_code)]
    pub fn with_model_path(model_path: &str) -> Result<Self, SummaryError> {
        let backend = LlamaBackend::init()
            .map_err(|e| SummaryError::InferenceFailed(format!("Failed to init backend: {}", e)))?;

        if !Path::new(model_path).exists() {
            return Err(SummaryError::ModelNotFound(format!(
                "Model not found at '{}'",
                model_path
            )));
        }

        Ok(Self {
            backend,
            model_path: model_path.into(),
        })
    }
}

impl Summarizer for LlamaCppSummarizer {
    fn summarize(&self, text: &str) -> Result<String, SummaryError> {
        // Загружаем модель
        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&self.backend, &self.model_path, &model_params)
            .map_err(|e| SummaryError::ModelNotFound(format!("Failed to load model: {}", e)))?;

        // Создаём контекст
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(CONTEXT_SIZE));
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| SummaryError::InferenceFailed(format!("Failed to create context: {}", e)))?;

        // Формируем промпт
        let prompt = format!(
            "<|user|>\n\
            Ты - помощник для суммаризации текста. \
            Создай краткое и информативное резюме следующего текста на русском языке. \
            Выдели ключевые моменты и основные идеи.\n\n\
            Текст:\n{}\n\n\
            <|assistant|>\n\
            Резюме:\n",
            text
        );

        // Токенизируем
        let tokens = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| SummaryError::InferenceFailed(format!("Tokenization failed: {}", e)))?;

        // Создаём batch
        let mut batch = LlamaBatch::new(CONTEXT_SIZE as usize, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(*token, i as i32, &[0], is_last)
                .map_err(|e| SummaryError::InferenceFailed(format!("Batch add failed: {}", e)))?;
        }

        // Декодируем промпт
        ctx.decode(&mut batch)
            .map_err(|e| SummaryError::InferenceFailed(format!("Decode failed: {}", e)))?;

        // Создаём sampler
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.3),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(42),
        ]);

        // Генерируем токены
        let mut result = String::new();
        let mut n_cur = tokens.len();

        for _ in 0..MAX_TOKENS {
            let token = sampler.sample(&ctx, -1);

            // Проверяем на EOS
            if model.is_eog_token(token) {
                break;
            }

            // Декодируем токен в текст
            let token_str = model
                .token_to_str(token, Special::Tokenize)
                .map_err(|e| SummaryError::InferenceFailed(format!("Token decode failed: {}", e)))?;

            result.push_str(&token_str);

            // Подготавливаем следующий batch
            batch.clear();
            batch.add(token, n_cur as i32, &[0], true)
                .map_err(|e| SummaryError::InferenceFailed(format!("Batch add failed: {}", e)))?;

            ctx.decode(&mut batch)
                .map_err(|e| SummaryError::InferenceFailed(format!("Decode failed: {}", e)))?;

            n_cur += 1;
        }

        Ok(result.trim().to_string())
    }
}
