use whisper_rs::{FullParams, WhisperContext, WhisperContextParameters, WhisperState};

/// Загрузить модель Whisper
pub fn load_model(path: &str) -> WhisperContext {
    println!("Загрузка модели: {}", path);
    WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .expect("Failed to load whisper model")
}

/// Создать параметры для распознавания
pub fn create_params() -> FullParams<'static, 'static> {
    let mut params = FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });

    params.set_language(Some("ru"));
    params.set_print_realtime(false);
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);

    // Анти-галлюцинации
    params.set_no_context(true);
    params.set_single_segment(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_temperature(0.0);

    params
}

/// Распознать аудио
pub fn transcribe(state: &mut WhisperState, audio: &[f32]) -> Vec<String> {
    let params = create_params();

    if state.full(params, audio).is_err() {
        eprintln!("Ошибка распознавания");
        return vec![];
    }

    let num_segments = state.full_n_segments();
    let mut results = Vec::new();
    let mut prev_text = String::new();

    for i in 0..num_segments {
        if let Some(text) = state.get_segment(i) {
            let text = text.to_str().unwrap().trim();

            // Фильтрация пустых, повторов и галлюцинаций
            if text.is_empty() || text == prev_text || is_hallucination(text) {
                continue;
            }

            results.push(text.to_string());
            prev_text = text.to_string();
        }
    }

    results
}

/// Проверка на типичные галлюцинации Whisper
fn is_hallucination(text: &str) -> bool {
    const HALLUCINATIONS: &[&str] = &[
        "Субтитры",
        "субтитры",
        "Подписывайтесь",
        "подписывайтесь",
        "Спасибо за просмотр",
        "www.",
        "http",
        "...",
        "♪",
        "Продолжение следует",
    ];

    HALLUCINATIONS.iter().any(|h| text.contains(h))
}
