use std::{sync::mpsc::{Receiver, Sender, channel}, thread::{JoinHandle, spawn}};

use cpal::{BuildStreamError, Device, Stream, traits::StreamTrait};

use crate::audio::{AudioCapture, AudioInitError, Event, ProcMsg, build_input_stream, find_device};



pub struct MacosAudioCapture {
    cmd_tx: Sender<ProcMsg>, 
    cmd_rx: Option<Receiver<ProcMsg>>,
    event_tx: Sender<Event>, 
    event_rx: Receiver<Event>,

    stream: Option<Stream>,
    blackhole: Device,
    record_handle: Option<JoinHandle<()>>
}

impl MacosAudioCapture {
    pub fn try_new() -> Result<Self, AudioInitError> {
        let (cmd_tx, cmd_rx): (Sender<ProcMsg>, Receiver<ProcMsg>) = channel();
        let (event_tx, event_rx): (Sender<Event>, Receiver<Event>) = channel();
        let host = cpal::default_host();

        let Some(device) = find_device(&host, String::from("BlackHole")) else {
            return Err(AudioInitError::BlackHoleNotFound);
        };

        Ok(Self { 
            cmd_tx: cmd_tx, 
            cmd_rx: Some(cmd_rx),
            event_rx: event_rx,
            event_tx: event_tx,

            stream: None,
            blackhole: device,
            record_handle: None
        })
    }
}

impl AudioCapture for MacosAudioCapture {
    fn start_record(&mut self) -> Result<(), BuildStreamError> {
        let cmd_rx = self.cmd_rx.take().expect("Recording alredy started!");

        let stream = match build_input_stream(&self.blackhole, self.cmd_tx.clone()) {
            Ok(s) => s,
            Err(err) => return Err(err)
        };

        // TODO: Обработка ошибки
        if stream.play().is_err() {
            eprintln!("Ошибка запуска stream");
        }   
        self.stream = Some(stream);

        let event_tx = self.event_tx.clone();
        let handel = spawn(move || {
            let mut buffer = Vec::new();

            while let Ok(data) = cmd_rx.recv() {
                match data {
                    ProcMsg::AudioSamples(data) => buffer.extend(&data),
                    ProcMsg::Stop => break
                }
            }

            // Stereo → Mono
            let mono: Vec<f32> = buffer
                .chunks(2)
                .map(|ch| (ch[0] + ch.get(1).unwrap_or(&0.0)) / 2.0)
                .collect();

            let _ = event_tx.send(Event::Finished(mono));
        });

        self.record_handle = Some(handel);
        Ok(())

    }
    fn stop_record(&mut self) -> Result<Vec<f32>,BuildStreamError> {
        let _ = self.cmd_tx.send(ProcMsg::Stop);
        let out = match self.event_rx.recv().expect("failed to stop_recorder") {
        Event::Finished(v) => v,
        Event::Error(err) => return Err(err)
        };

        if let Some(h) = self.record_handle.take() {
            let _ = h.join();
        }

        Ok(out)
    }
}
