use super::{SummaryError, Summarizer};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "http://localhost:8080/v1/chat/completions";
const REQUEST_TIMEOUT_SECS: u64 = 120;

pub struct MlxSummarizer {
    client: reqwest::blocking::Client,
    endpoint: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: String,
}

impl MlxSummarizer {
    pub fn new() -> Result<Self, SummaryError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SummaryError::InferenceFailed(e.to_string()))?;

        Ok(Self {
            client,
            endpoint: DEFAULT_ENDPOINT.into(),
        })
    }

    /// Создаёт MlxSummarizer с кастомным endpoint
    pub fn with_endpoint(endpoint: &str) -> Result<Self, SummaryError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| SummaryError::InferenceFailed(e.to_string()))?;

        Ok(Self {
            client,
            endpoint: endpoint.into(),
        })
    }
}

impl Summarizer for MlxSummarizer {
    fn summarize(&self, text: &str) -> Result<String, SummaryError> {
        let prompt = format!(
            "Ты - помощник для суммаризации текста. \
            Создай краткое и информативное резюме следующего текста на русском языке. \
            Выдели ключевые моменты и основные идеи.\n\n\
            Текст:\n{}\n\n\
            Резюме:",
            text
        );

        let request = ChatRequest {
            model: "default".into(),
            messages: vec![Message {
                role: "user".into(),
                content: prompt,
            }],
            max_tokens: 1024,
            temperature: 0.3,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .map_err(|e| {
                if e.is_connect() {
                    SummaryError::ServerUnavailable(format!(
                        "MLX server not running. Start with: mlx_lm.server --model mlx-community/Phi-3-mini-4k-instruct-4bit\nError: {}",
                        e
                    ))
                } else {
                    SummaryError::InferenceFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(SummaryError::InferenceFailed(format!(
                "Server returned status: {}",
                response.status()
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .map_err(|e| SummaryError::InferenceFailed(e.to_string()))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| SummaryError::InferenceFailed("Empty response from model".into()))
    }
}
