mod audio;
mod resample;
mod summary;
mod whisper;

use std::{fs::File, sync::mpsc::channel};
use std::io::{Read, Write};
use resample::resample_audio;

const MODEL_PATH: &str = "models/ggml-medium.bin";
const SAMPLE_RATE_INPUT: usize = 48000;
const SAMPLE_RATE_WHISPER: usize = 16000;

fn main() {
    record();
}


fn record () {
    // Запись аудио
    let mut audio_capture = audio::make_audio_capture().unwrap();
    println!("START RECORDING");
    let (tx, rx) = channel();
    audio_capture.start_record().unwrap();
    ctrlc::set_handler(move || {
        println!("STOP RECORD");
        audio_capture.stop_record().unwrap();
        tx.send(()).unwrap();
    }).unwrap();

    rx.recv().unwrap();
}
fn stt() {
    let mut reader = hound::WavReader::open("temp.wav").unwrap();
    let samples: Vec<f32> = reader
        .samples::<f32>()
        .map(|s| s.unwrap())
        .collect();
    if samples.is_empty() {
        eprintln!("Нет аудио данных!");
        return;
    }

    // Resample 48kHz → 16kHz
    let resampled = resample_audio(&samples, SAMPLE_RATE_INPUT, SAMPLE_RATE_WHISPER);

    // агрузка модели
    let whisper_ctx = whisper::load_model(MODEL_PATH);
    let mut state = whisper_ctx
        .create_state()
        .expect("Failed to create whisper state");

    // Распознавание
    let segments = whisper::transcribe(&mut state, &resampled);

    match File::create("stt_result.txt") {
        Ok(mut stt_output) => {
            for text in &segments {
                writeln!(stt_output, "{}", text).unwrap();
            }
        }
        Err(_) => {}
    }
}

fn summarize(text: &str) -> Result<(), summary::SummaryError> {
    let mut full_text = String::new();
    match File::open("stt_result.txt") {
        Ok(mut stt_file) => {
            stt_file.read_to_string(&mut full_text).unwrap();        
        }
        Err(_) => {}
    }
    if let Err(e) = summarize(&full_text) {
        eprintln!("Ошибка суммаризации: {}", e);
    }
    println!();
    println!("=== Суммаризация ===");

    let summarizer = summary::create_summarizer()?;
    let result = summarizer.summarize(text)?;

    println!("{}", result);
    Ok(())
}
