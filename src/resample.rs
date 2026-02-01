use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

pub fn resample_audio(input: &[f32], from_rate: usize, to_rate: usize) -> Vec<f32> {
    let chunk_size = 2048;

    let mut resampler = Fft::<f32>::new(
        from_rate,
        to_rate,
        chunk_size,
        1, // sub_chunks
        1, // channels
        FixedSync::Input,
    )
    .expect("Failed to create resampler");

    // Считаем размер output
    let output_frames = (input.len() as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
    let mut output = vec![0.0f32; output_frames + chunk_size];

    let mut input_offset = 0;
    let mut output_offset = 0;

    while input_offset < input.len() {
        let remaining = input.len() - input_offset;
        let frames_to_process = remaining.min(chunk_size);

        // Pad если нужно
        let mut chunk = vec![0.0f32; chunk_size];
        chunk[..frames_to_process]
            .copy_from_slice(&input[input_offset..input_offset + frames_to_process]);

        let input_adapter = InterleavedSlice::new(&chunk, 1, chunk_size).unwrap();
        let output_slice = &mut output[output_offset..];
        let out_frames = output_slice
            .len()
            .min(chunk_size * to_rate / from_rate + 10);
        let mut output_adapter =
            InterleavedSlice::new_mut(&mut output_slice[..out_frames], 1, out_frames).unwrap();

        if let Ok((_, written)) =
            resampler.process_into_buffer(&input_adapter, &mut output_adapter, None)
        {
            output_offset += written;
        }

        input_offset += frames_to_process;
    }

    output.truncate(output_offset);
    output
}
