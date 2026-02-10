use std::{
    sync::mpsc::{Receiver, Sender, channel},
    thread::{JoinHandle, spawn},
};

use hound::WavWriter;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Audio capture is not supported on this platform")]
    UnsupportedPlatform,

    #[error("Failed to initialize audio backend")]
    Init(#[from] AudioInitError),
}

#[derive(Debug, Error)]
pub enum AudioInitError {
    #[error("No suitable audio device found")]
    DeviceNotFound,
    #[error("ScreenCaptureKit error: {0}")]
    ScreenCapture(String),
    #[error("PulseAudio device not found")]
    PulseAudioNotFound,
}

#[derive(Debug)]
pub enum ProcMsg {
    SystemAudio(Vec<f32>),
    MicrophoneAudio(Vec<f32>),
    Stop,
}

#[derive(Debug)]
pub enum Event {
    Finished,
}

pub trait AudioCapture {
    fn start_record(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn stop_record(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

pub fn make_audio_capture() -> Result<Box<dyn AudioCapture + Send>, AudioInitError> {
    #[cfg(target_os = "macos")]
    {
        let cap = MacOSAudioCapture::new()?;
        Ok(Box::new(cap))
    }

    #[cfg(target_os = "linux")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        if let Some(device) = find_device("PulseAudio") {
            let cap = CpalAudioCapture::new(device);
            Ok(Box::new(cap))
        } else {
            Err(AudioInitError::PulseAudioNotFound)
        }
    }
}

// ============================================================================
// macOS: ScreenCaptureKit (системный звук + микрофон, macOS 15+)
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use screencapturekit::prelude::*;

    // --- Handler для системного аудио и микрофона ---

    struct AudioHandler {
        tx: Sender<ProcMsg>,
    }

    /// Конвертирует planar аудио из ScreenCaptureKit в interleaved stereo
    fn extract_samples(sample: &CMSampleBuffer) -> Option<Vec<f32>> {
        let buf_list = sample.audio_buffer_list()?;
        let num_buffers = buf_list.num_buffers();

        let samples = if num_buffers == 2 {
            // ScreenCaptureKit возвращает planar формат (2 буфера по 1 каналу)
            // Конвертируем в interleaved stereo: [L,R,L,R,...]
            let buf0 = buf_list.get(0)?;
            let buf1 = buf_list.get(1)?;

            let left: &[f32] = unsafe {
                std::slice::from_raw_parts(buf0.data().as_ptr() as *const f32, buf0.data().len() / 4)
            };
            let right: &[f32] = unsafe {
                std::slice::from_raw_parts(buf1.data().as_ptr() as *const f32, buf1.data().len() / 4)
            };

            let mut interleaved = Vec::with_capacity(left.len() + right.len());
            for i in 0..left.len().min(right.len()) {
                interleaved.push(left[i]);
                interleaved.push(right[i]);
            }
            interleaved
        } else {
            // Fallback: моно или другой формат
            let mut all = Vec::new();
            for i in 0..num_buffers {
                if let Some(buf) = buf_list.get(i) {
                    let bytes = buf.data();
                    if !bytes.is_empty() {
                        let data: &[f32] = unsafe {
                            std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4)
                        };
                        all.extend_from_slice(data);
                    }
                }
            }
            all
        };

        if samples.is_empty() {
            None
        } else {
            Some(samples)
        }
    }

    impl SCStreamOutputTrait for AudioHandler {
        fn did_output_sample_buffer(
            &self,
            sample: CMSampleBuffer,
            output_type: SCStreamOutputType,
        ) {
            if let Some(samples) = extract_samples(&sample) {
                let msg = match output_type {
                    SCStreamOutputType::Audio => ProcMsg::SystemAudio(samples),
                    SCStreamOutputType::Microphone => ProcMsg::MicrophoneAudio(samples),
                    _ => return,
                };
                let _ = self.tx.send(msg);
            }
        }
    }

    // --- Основная структура для macOS ---

    pub struct MacOSAudioCapture {
        event_tx: Sender<Event>,
        event_rx: Receiver<Event>,
        sc_stream: Option<SCStream>,
        writer_handle: Option<JoinHandle<()>>,
    }

    impl MacOSAudioCapture {
        pub fn new() -> Result<Self, AudioInitError> {
            let (event_tx, event_rx) = channel();

            Ok(Self {
                event_tx,
                event_rx,
                sc_stream: None,
                writer_handle: None,
            })
        }
    }

    impl AudioCapture for MacOSAudioCapture {
        fn start_record(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            // --- 1. Настраиваем ScreenCaptureKit ---
            let content = SCShareableContent::get()
                .map_err(|e| AudioInitError::ScreenCapture(format!("{:?}", e)))?;

            let display = content
                .displays()
                .into_iter()
                .next()
                .ok_or(AudioInitError::ScreenCapture("No displays found".into()))?;

            let filter = SCContentFilter::create()
                .with_display(&display)
                .with_excluding_windows(&[])
                .build();

            let config = SCStreamConfiguration::new()
                .with_width(1920)
                .with_height(1080)
                .with_captures_audio(true)
                .with_captures_microphone(true)
                .with_sample_rate(48000)
                .with_channel_count(2);

            // Два отдельных канала для избежания блокировки
            let (sys_tx, sys_rx): (Sender<ProcMsg>, Receiver<ProcMsg>) = channel();
            let (mic_tx, mic_rx): (Sender<ProcMsg>, Receiver<ProcMsg>) = channel();

            let sys_handler = AudioHandler { tx: sys_tx };
            let mic_handler = AudioHandler { tx: mic_tx };

            let mut stream = SCStream::new(&filter, &config);
            stream.add_output_handler(sys_handler, SCStreamOutputType::Audio);
            stream.add_output_handler(mic_handler, SCStreamOutputType::Microphone);

            stream
                .start_capture()
                .map_err(|e| AudioInitError::ScreenCapture(format!("{:?}", e)))?;

            println!("Audio capture started (system + microphone → mixed mono)");
            self.sc_stream = Some(stream);

            // --- 2. Поток записи WAV ---
            let spec = hound::WavSpec {
                channels: 1,  // Моно для простоты микширования
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = WavWriter::create("temp.wav", spec)?;

            let event_tx = self.event_tx.clone();

            let writer_handle = spawn(move || {
                use std::sync::mpsc::TryRecvError;

                let mut sys_buffer = Vec::new();
                let mut mic_buffer = Vec::new();
                let mut running = true;
                let mut sys_count = 0;
                let mut mic_count = 0;

                while running {
                    // Читаем из обоих каналов неблокирующе
                    match sys_rx.try_recv() {
                        Ok(ProcMsg::SystemAudio(data)) => {
                            sys_count += 1;
                            // Система приходит как stereo interleaved [L,R,L,R,...]
                            // Конвертируем в моно: (L+R)/2
                            for chunk in data.chunks_exact(2) {
                                let mono = (chunk[0] + chunk[1]) * 0.5;
                                sys_buffer.push(mono);
                            }
                        }
                        Ok(ProcMsg::Stop) => running = false,
                        Err(TryRecvError::Empty) => {}
                        Err(TryRecvError::Disconnected) => running = false,
                        _ => {}
                    }

                    match mic_rx.try_recv() {
                        Ok(ProcMsg::MicrophoneAudio(data)) => {
                            mic_count += 1;
                            // Микрофон уже моно, добавляем как есть
                            mic_buffer.extend_from_slice(&data);
                        }
                        Ok(ProcMsg::Stop) => running = false,
                        Err(TryRecvError::Empty) => {}
                        Err(TryRecvError::Disconnected) => running = false,
                        _ => {}
                    }

                    // Микшируем доступные данные
                    let mix_len = sys_buffer.len().min(mic_buffer.len());
                    if mix_len > 0 {
                        for i in 0..mix_len {
                            let mixed = (sys_buffer[i] + mic_buffer[i]) * 0.5;
                            let sample_i16 = (mixed.clamp(-1.0, 1.0) * 32767.0) as i16;
                            let _ = writer.write_sample(sample_i16);
                        }
                        sys_buffer.drain(0..mix_len);
                        mic_buffer.drain(0..mix_len);
                    } else if !sys_buffer.is_empty() || !mic_buffer.is_empty() {
                        // Если один буфер пустой, ждём немного
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    } else {
                        // Оба буфера пусты, ждём данных
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }

                // Дописываем остатки
                let remaining = sys_buffer.len().max(mic_buffer.len());
                for i in 0..remaining {
                    let sys = sys_buffer.get(i).copied().unwrap_or(0.0);
                    let mic = mic_buffer.get(i).copied().unwrap_or(0.0);
                    let mixed = (sys + mic) * 0.5;
                    let sample_i16 = (mixed.clamp(-1.0, 1.0) * 32767.0) as i16;
                    let _ = writer.write_sample(sample_i16);
                }

                let _ = writer.finalize();
                let _ = event_tx.send(Event::Finished);
            });
            self.writer_handle = Some(writer_handle);

            Ok(())
        }

        fn stop_record(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            // 1. Останавливаем ScreenCaptureKit
            if let Some(stream) = self.sc_stream.take() {
                let _ = stream.stop_capture();
                println!("Audio capture stopped");
            }

            // 2. Даём время на flush оставшихся данных из SCK
            std::thread::sleep(std::time::Duration::from_millis(200));

            // 3. Ждём завершения writer (он завершится когда audio_rx закроется)
            if let Ok(Event::Finished) = self.event_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                println!("WAV file saved");
            }

            // 4. Ждём завершения потока
            if let Some(h) = self.writer_handle.take() {
                let _ = h.join();
            }

            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::MacOSAudioCapture;

// ============================================================================
// Linux: cpal (PulseAudio) — оставляем как было
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{BuildStreamError, Device, Stream, StreamConfig, StreamError};

    pub struct CpalAudioCapture {
        wav_tx: Sender<ProcMsg>,
        wav_rx: Option<Receiver<ProcMsg>>,
        event_tx: Sender<Event>,
        event_rx: Receiver<Event>,
        stream: Option<Stream>,
        device: Device,
        writer_handle: Option<JoinHandle<()>>,
    }

    impl CpalAudioCapture {
        pub fn new(device: Device) -> Self {
            let (wav_tx, wav_rx) = channel();
            let (event_tx, event_rx) = channel();
            Self {
                wav_tx,
                wav_rx: Some(wav_rx),
                event_tx,
                event_rx,
                stream: None,
                device,
                writer_handle: None,
            }
        }
    }

    impl AudioCapture for CpalAudioCapture {
        fn start_record(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let wav_rx = self.wav_rx.take().expect("Recording already started!");

            let config = self.device.default_input_config()?;
            let num_channels = config.channels() as usize;
            let stream_config = StreamConfig {
                buffer_size: cpal::BufferSize::Default,
                channels: config.channels(),
                sample_rate: config.sample_rate(),
            };

            let tx = self.wav_tx.clone();
            let stream = self.device.build_input_stream(
                &stream_config,
                move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    let mono = mix_to_mono(data, num_channels);
                    let _ = tx.send(ProcMsg::AudioSamples(mono));
                },
                |err: StreamError| eprintln!("Stream error: {}", err),
                None,
            )?;

            stream.play()?;
            self.stream = Some(stream);

            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = WavWriter::create("temp.wav", spec)?;

            let event_tx = self.event_tx.clone();
            let handle = spawn(move || {
                while let Ok(msg) = wav_rx.recv() {
                    match msg {
                        ProcMsg::AudioSamples(data) => {
                            for sample in data {
                                let _ = writer.write_sample(sample);
                            }
                        }
                        ProcMsg::Stop => break,
                    }
                }
                let _ = event_tx.send(Event::Finished);
            });
            self.writer_handle = Some(handle);

            Ok(())
        }

        fn stop_record(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            if let Some(stream) = self.stream.take() {
                let _ = stream.pause();
                drop(stream);
            }

            let _ = self.wav_tx.send(ProcMsg::Stop);
            if let Ok(Event::Finished) = self.event_rx.recv() {}

            if let Some(h) = self.writer_handle.take() {
                let _ = h.join();
            }

            Ok(())
        }
    }

    pub fn find_device(name: &str) -> Option<Device> {
        let host = cpal::default_host();
        let devices: Vec<_> = host.input_devices().ok()?.collect();
        devices.into_iter().find(|d| {
            d.description()
                .map(|desc| desc.name().contains(name))
                .unwrap_or(false)
        })
    }
}

#[cfg(target_os = "linux")]
pub use linux::{CpalAudioCapture, find_device};

// ============================================================================
// Общие утилиты
// ============================================================================
