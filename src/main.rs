use std::{thread::sleep, time::Duration};

use cpal::{
    Device, Host, InputCallbackInfo, StreamConfig, StreamError,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

fn main() {
    let host = cpal::default_host();

    if let Some(micro) = get_default_micro(&host) {
        let config = micro.default_input_config().unwrap();
        println!(
            "{}\n{}\n{}",
            micro.id().unwrap(),
            config.sample_rate(),
            config.channels()
        );
        let micro_stream = get_micro_stream(&micro).unwrap();
        micro_stream.play();
        sleep(Duration::new(3, 0));
    }
}

fn get_default_micro(host: &Host) -> Option<Device> {
    let device = host.default_input_device();
    return device;
}

fn get_micro_stream(micro: &Device) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let config = micro.default_input_config().unwrap();
    let stream_config = StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: config.channels(),
        sample_rate: config.sample_rate(),
    };
    return micro.build_input_stream(&stream_config, data_callback, error_callback, None);
}

fn data_callback(data: &[f32], _info: &InputCallbackInfo) {
    let volume: f32 = data.iter().map(|s| s.abs()).sum::<f32>() / data.len() as f32;
    let bars = (volume * 50.0) as usize; // масштаб                                                                                                  
    let bar = "#".repeat(bars);
    println!("[{:<50}] {:.2}", bar, volume);
}

fn error_callback(stream_error: StreamError) {}
