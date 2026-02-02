use std::{
    sync::mpsc::{Sender},
};

use thiserror::Error;

use cpal::{
    BuildStreamError, Device, Host, InputCallbackInfo, StreamConfig, StreamError, traits::{DeviceTrait, HostTrait}
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
}

#[derive(Debug)]
pub enum ProcMsg {
    AudioSamples(Vec<f32>),
    Stop,                // завершить поток
}

#[derive(Debug)]
pub enum Event {
    Error(BuildStreamError),
    Finished(Vec<f32>)
}

pub trait AudioCapture {
    fn start_record(&mut self) ->Result<(), BuildStreamError>;
    fn stop_record(&mut self) -> Result<Vec<f32>,BuildStreamError>;
}

pub fn make_audio_capture() -> Result<Box<dyn AudioCapture>, AudioError> {
    #[cfg(target_os="linux")]
    {
        use crate::audio::LinuxAudioCapture;
        let cap = LinuxAudioCapture::try_new()?;
        Ok(Box::new(cap))
    }

    #[cfg(target_os="macos")]
    {
        use crate::audio::MacosAudioCapture;
        let cap = MacosAudioCapture::try_new()?;
        Ok(Box::new(cap))
    }
}

pub fn find_device(host: &Host, name: String) -> Option<Device> {
    host.input_devices().ok()?.find(|d| {
        d.description()
            .map(|desc| desc.name().contains(&name))
            .unwrap_or(false)
    })
}

fn find_blackhole(host: &Host) -> Option<Device> {
    host.input_devices().ok()?.find(|d| {
        d.description()
            .map(|desc| desc.name().contains("BlackHole"))
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
            let _ = tx.send(ProcMsg::AudioSamples(data.to_vec()));
        },
        |err: StreamError| eprintln!("Stream error: {}", err),
        None,
    )
}
