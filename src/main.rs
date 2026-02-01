mod audio;
mod resample;
mod whisper;

use resample::resample_audio;

const MODEL_PATH: &str = "models/ggml-large-v3.bin";
const SAMPLE_RATE_INPUT: usize = 48000;
const SAMPLE_RATE_WHISPER: usize = 16000;
const RECORD_DURATION_SECS: u64 = 120;

fn main() {
    // 1. Загрузка модели
    let whisper_ctx = whisper::load_model(MODEL_PATH);
    let mut state = whisper_ctx
        .create_state()
        .expect("Failed to create whisper state");

    // 2. Запись аудио
    let audio = audio::record(RECORD_DURATION_SECS);
    if audio.is_empty() {
        eprintln!("Нет аудио данных!");
        return;
    }

    // 3. Resample 48kHz → 16kHz
    let resampled = resample_audio(&audio, SAMPLE_RATE_INPUT, SAMPLE_RATE_WHISPER);

    // 4. Распознавание
    let segments = whisper::transcribe(&mut state, &resampled);

    // 5. Вывод результатов
    println!();
    println!("=== Результат ===");
    for text in &segments {
        println!("> {}", text);
    }
}
