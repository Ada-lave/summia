mod audio;
mod summary;

use std::io::Write;
use std::{fs::File, sync::mpsc::channel};

use fluidaudio_rs::FluidAudio;

fn main() {
    record();
}

fn record() {
    let mut audio_capture = audio::make_audio_capture().unwrap();
    println!("START RECORDING");
    let (tx, rx) = channel();
    audio_capture.start_record().unwrap();
    ctrlc::set_handler(move || {
        println!("STOP RECORD");
        audio_capture.stop_record().unwrap();
        tx.send(()).unwrap();
    })
    .unwrap();

    rx.recv().unwrap();
}

fn stt() {
    let audio = FluidAudio::new().expect("Failed to create FluidAudio");
    audio.init_asr().expect("Failed to initialize ASR");

    let result = audio
        .transcribe_file("temp.wav")
        .expect("Failed to transcribe");

    println!("Transcription: {}", result.text);
    println!("Confidence: {:.1}%", result.confidence * 100.0);
    println!("Duration: {:.2}s", result.duration);

    match File::create("stt_result.txt") {
        Ok(mut stt_output) => {
            writeln!(stt_output, "{}", result.text).unwrap();
        }
        Err(e) => eprintln!("Failed to write stt_result.txt: {}", e),
    }
}

fn summarize(text: &str) -> Result<(), summary::SummaryError> {
    println!("\n=== Суммаризация ===");

    let summarizer = summary::create_summarizer()?;
    let result = summarizer.summarize(text)?;

    println!("{}", result);
    Ok(())
}
