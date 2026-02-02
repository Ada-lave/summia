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

pub fn make_audio_capture() -> Result<Box<dyn AudioCapture>, AudioInitError> {
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
        if let Some(device) = find_device("BlackHole") {
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

pub fn build_input_stream(
    device: &Device,
    tx: Sender<ProcMsg>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let config = device.default_input_config().unwrap();
    let stream_config = StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: config.channels(),
        sample_rate: config.sample_rate(),
    };

    device.build_input_stream(
        &stream_config,
        move |data: &[f32], _info: &InputCallbackInfo| {
            let _ = tx.send(ProcMsg::AudioSamples(
                data.to_vec()
                    .chunks(2)
                    .map(|ch| (ch[0] + ch.get(1).unwrap_or(&0.0)) / 2.0)
                    .collect(),
            ));
        },
        |err: StreamError| eprintln!("Stream error: {}", err),
        None,
    )
}
