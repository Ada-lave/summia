use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use cpal::{
    Device, Host, InputCallbackInfo, StreamConfig, StreamError,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

/// Записать аудио с BlackHole
pub fn record(duration_secs: u64) -> Vec<f32> {
    let host = cpal::default_host();

    let Some(device) = find_blackhole(&host) else {
        eprintln!("BlackHole не найден! Установи: brew install blackhole-2ch");
        return vec![];
    };

    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_default();
    println!("Устройство: {}", device_name);

    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = mpsc::channel();

    let stream = match build_input_stream(&device, tx) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Ошибка создания stream: {}", e);
            return vec![];
        }
    };

    if stream.play().is_err() {
        eprintln!("Ошибка запуска stream");
        return vec![];
    }

    println!("Запись {} сек...", duration_secs);

    let mut buffer = Vec::new();
    let start = Instant::now();
    let duration = Duration::from_secs(duration_secs);

    while let Ok(data) = rx.recv_timeout(Duration::from_secs(5)) {
        buffer.extend(&data);

        if start.elapsed() >= duration {
            break;
        }
    }

    // Stereo → Mono
    let mono: Vec<f32> = buffer
        .chunks(2)
        .map(|ch| (ch[0] + ch.get(1).unwrap_or(&0.0)) / 2.0)
        .collect();

    println!("Записано: {} сэмплов", mono.len());
    mono
}

fn find_blackhole(host: &Host) -> Option<Device> {
    host.input_devices().ok()?.find(|d| {
        d.description()
            .map(|desc| desc.name().contains("BlackHole"))
            .unwrap_or(false)
    })
}

fn build_input_stream(
    device: &Device,
    tx: Sender<Vec<f32>>,
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
            let _ = tx.send(data.to_vec());
        },
        |err: StreamError| eprintln!("Stream error: {}", err),
        None,
    )
}
