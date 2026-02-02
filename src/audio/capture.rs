use std::{
    sync::mpsc::{Receiver, Sender, channel},
    thread::{JoinHandle, spawn},
};

use hound::WavWriter;
use thiserror::Error;

use cpal::traits::StreamTrait;
use cpal::{
    BuildStreamError, Device, InputCallbackInfo, Stream, StreamConfig, StreamError,
    traits::{DeviceTrait, HostTrait},
};

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Audio capture is not supported on this platform")]
    UnsupportedPlatform,

    #[error("Failed to initialize audio backend")]
    Init(#[from] AudioInitError),
}

#[derive(Debug, Error)]
pub enum AudioInitError {
    #[error("BlackHole device not found (install: brew install blackhole-2ch)")]
    BlackHoleNotFound,
    #[error("PulseAudioNotFound device not found")]
    PulseAudioNotFound,
}

#[derive(Debug)]
pub enum ProcMsg {
    AudioSamples(Vec<f32>),
    Stop, // завершить поток
}

#[derive(Debug)]
pub enum Event {
    Error(BuildStreamError),
    Finished,
}

pub trait AudioCapture {
    fn start_record(&mut self) -> Result<(), BuildStreamError>;
    fn stop_record(&mut self) -> Result<(), BuildStreamError>;
}

pub fn make_audio_capture() -> Result<Box<dyn AudioCapture + Send>, AudioInitError> {
    #[cfg(target_os = "linux")]
    {
        if let Some(device) = find_device("PulseAudio") {
            let cap = AudioCaptureImpl::try_new(device)?;
            Ok(Box::new(cap))
        } else {
            return Err(AudioInitError::PulseAudioNotFound);
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Сначала пробуем LoopBack (Aggregate Device), потом BlackHole
        if let Some(device) = find_device("LoopBack2") {
            let cap = AudioCaptureImpl::try_new(device)?;
            Ok(Box::new(cap))
        } else if let Some(device) = find_device("BlackHole") {
            let cap = AudioCaptureImpl::try_new(device)?;
            Ok(Box::new(cap))
        } else {
            return Err(AudioInitError::BlackHoleNotFound);
        }
    }
}

pub struct AudioCaptureImpl {
    cmd_tx: Sender<ProcMsg>,
    cmd_rx: Option<Receiver<ProcMsg>>,
    event_tx: Sender<Event>,
    event_rx: Receiver<Event>,

    stream: Option<Stream>,
    device: Device,
    record_handle: Option<JoinHandle<()>>,
}

impl AudioCaptureImpl {
    pub fn try_new(device: Device) -> Result<Self, AudioInitError> {
        let (cmd_tx, cmd_rx): (Sender<ProcMsg>, Receiver<ProcMsg>) = channel();
        let (event_tx, event_rx): (Sender<Event>, Receiver<Event>) = channel();

        Ok(Self {
            cmd_tx: cmd_tx,
            cmd_rx: Some(cmd_rx),
            event_rx: event_rx,
            event_tx: event_tx,

            stream: None,
            device: device,
            record_handle: None,
        })
    }
}

impl AudioCapture for AudioCaptureImpl {
    fn start_record(&mut self) -> Result<(), BuildStreamError> {
        let cmd_rx = self.cmd_rx.take().expect("Recording alredy started!");

        let stream = match build_input_stream(&self.device, self.cmd_tx.clone()) {
            Ok(s) => s,
            Err(err) => return Err(err),
        };

        // TODO: Обработка ошибки
        if stream.play().is_err() {
            eprintln!("Ошибка запуска stream");
        }
        self.stream = Some(stream);

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create("temp.wav", spec).unwrap();

        let event_tx = self.event_tx.clone();
        let handel = spawn(move || {
            while let Ok(data) = cmd_rx.recv() {
                match data {
                    ProcMsg::AudioSamples(data) => {
                        for sample in data {
                            writer.write_sample(sample).unwrap()
                        }
                    }
                    ProcMsg::Stop => break,
                }
            }

            let _ = event_tx.send(Event::Finished);
        });

        self.record_handle = Some(handel);
        Ok(())
    }
    fn stop_record(&mut self) -> Result<(), BuildStreamError> {
        // Сначала останавливаем stream, чтобы он перестал отправлять данные
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
            drop(stream);
        }

        // Теперь отправляем Stop - он точно дойдёт до потока
        let _ = self.cmd_tx.send(ProcMsg::Stop);
        match self.event_rx.recv().expect("failed to stop_recorder") {
            Event::Finished => Ok(()),
            Event::Error(err) => Err(err),
        }?;

        if let Some(h) = self.record_handle.take() {
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

/// Микширует interleaved аудио в моно
fn mix_to_mono(interleaved: &[f32], num_channels: usize) -> Vec<f32> {
    if num_channels == 0 {
        return Vec::new();
    }
    interleaved
        .chunks(num_channels)
        .map(|ch| ch.iter().sum::<f32>() / num_channels as f32)
        .collect()
}

pub fn build_input_stream(
    device: &Device,
    tx: Sender<ProcMsg>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    // Показываем все доступные конфигурации устройства
    println!("Device: {:?}", device.description());
    if let Ok(configs) = device.supported_input_configs() {
        println!("Supported input configurations:");
        for cfg in configs {
            println!("  - channels: {}", cfg.channels());
        }
    }

    // Получаем максимальное количество каналов
    let max_channels = device
        .supported_input_configs()
        .ok()
        .and_then(|configs| configs.map(|c| c.channels()).max())
        .unwrap_or(2);

    let config = device.default_input_config().unwrap();
    let sample_rate = config.sample_rate();

    // Используем максимальное количество каналов вместо дефолтного
    let num_channels = max_channels as usize;
    let stream_config = StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: max_channels,
        sample_rate,
    };

    println!(
        "Recording with {} channels (default was {})",
        num_channels,
        config.channels()
    );

    device.build_input_stream(
        &stream_config,
        move |data: &[f32], _info: &InputCallbackInfo| {
            let mono = mix_to_mono(data, num_channels);
            let _ = tx.send(ProcMsg::AudioSamples(mono));
        },
        |err: StreamError| eprintln!("Stream error: {}", err),
        None,
    )
}
