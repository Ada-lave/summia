use std::sync::mpsc::{Receiver, Sender, channel};

use cpal::BuildStreamError;

use crate::audio::{AudioCapture, AudioInitError};



pub struct LinuxAudioCapture {
    audio_buffer: Vec<f32>,
    audio_buffer_sender: Sender<Vec<f32>>, 
    audio_buffer_reciver: Receiver<Vec<f32>>
}

impl LinuxAudioCapture {
    pub fn try_new() -> Result<Self, AudioInitError> {
        let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
        
        Ok(Self {
            audio_buffer: Vec::new(), 
            audio_buffer_sender: tx, 
            audio_buffer_reciver: rx 
        })
    }
}

impl AudioCapture for LinuxAudioCapture {
    fn start_record(&mut self) -> Result<(), BuildStreamError>{
        return Ok(());
    }
    fn stop_record(&mut self) -> Result<Vec<f32>,BuildStreamError> {
        return Ok(Vec::new());
    }
}