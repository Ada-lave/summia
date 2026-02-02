mod audio;
mod resample;
mod whisper;

use std::{thread::sleep, time::Duration};

use resample::resample_audio;

const MODEL_PATH: &str = "models/ggml-medium.bin";
const SAMPLE_RATE_INPUT: usize = 48000;
const SAMPLE_RATE_WHISPER: usize = 16000;

fn main() {
    // 1. Запись аудио
    let mut audio_capture = audio::make_audio_capture().unwrap();
    println!("START RECORDING");
    audio_capture.start_record().unwrap();
    sleep(Duration::from_secs(180));
    let samples = audio_capture.stop_record().unwrap();
    if samples.is_empty() {
        eprintln!("Нет аудио данных!");
        return;
    }
    println!("STOP RECORD");
    // 2. Resample 48kHz → 16kHz
    let resampled = resample_audio(&samples, SAMPLE_RATE_INPUT, SAMPLE_RATE_WHISPER);

    // 3. Загрузка модели
    let whisper_ctx = whisper::load_model(MODEL_PATH);
    let mut state = whisper_ctx
        .create_state()
        .expect("Failed to create whisper state");

    // 4. Распознавание
    let segments = whisper::transcribe(&mut state, &resampled);

    // 5. Вывод результатов
    println!();
    println!("=== Результат ===");
    for text in &segments {
        println!("> {}", text);
    }
}
