use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::{Arc, Mutex};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub const VISUALIZER_BARS: usize = 12;

#[cfg(target_os = "macos")]
const MICROPHONE_HELP: &str =
    "Open System Settings → Privacy & Security → Microphone, enable Quill, then try again.";
#[cfg(not(target_os = "macos"))]
const MICROPHONE_HELP: &str =
    "Check that a microphone is connected and allowed for Quill, then try again.";

pub struct AudioSnapshot {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}

pub struct AudioCapture {
    _stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    visual_levels: Arc<Mutex<[f32; VISUALIZER_BARS]>>,
    source_sample_rate: u32,
    error: Arc<Mutex<Option<String>>>,
    pub device_name: String,
}

pub fn input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut devices = host
        .input_devices()
        .with_context(|| format!("Could not enumerate audio input devices. {MICROPHONE_HELP}"))?
        .filter_map(|device| device.name().ok())
        .collect::<Vec<_>>();
    devices.sort();
    devices.dedup();
    Ok(devices)
}

impl AudioCapture {
    pub fn start(selected_device: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(selected) = selected_device {
            host.input_devices()
                .with_context(|| {
                    format!("Could not enumerate audio input devices. {MICROPHONE_HELP}")
                })?
                .find(|device| device.name().is_ok_and(|name| name == selected))
                .ok_or_else(|| anyhow!("configured microphone is unavailable: {selected}"))?
        } else {
            host.default_input_device()
                .with_context(|| format!("No default microphone is available. {MICROPHONE_HELP}"))?
        };
        let device_name = device
            .name()
            .unwrap_or_else(|_| "Unknown microphone".into());
        let supported = device.default_input_config().with_context(|| {
            format!("Could not read the format for microphone '{device_name}'. {MICROPHONE_HELP}")
        })?;
        let source_sample_rate = supported.sample_rate().0;
        let config: StreamConfig = supported.clone().into();
        let channels = config.channels as usize;
        let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
            source_sample_rate as usize * 30,
        )));
        let visual_levels = Arc::new(Mutex::new([0.0; VISUALIZER_BARS]));
        let error = Arc::new(Mutex::new(None::<String>));
        let error_sink = Arc::clone(&error);
        let on_error = move |stream_error: cpal::StreamError| {
            if let Ok(mut current) = error_sink.lock() {
                *current = Some(stream_error.to_string());
            }
        };

        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let sink = Arc::clone(&samples);
                let meter = Arc::clone(&visual_levels);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            append_mono(&sink, &meter, data, channels, |value| value)
                        },
                        on_error,
                        None,
                    )
                    .with_context(|| {
                        format!("Could not open microphone '{device_name}'. {MICROPHONE_HELP}")
                    })?
            }
            SampleFormat::I16 => {
                let sink = Arc::clone(&samples);
                let meter = Arc::clone(&visual_levels);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _| {
                            append_mono(&sink, &meter, data, channels, |value| {
                                value as f32 / 32_768.0
                            })
                        },
                        on_error,
                        None,
                    )
                    .with_context(|| {
                        format!("Could not open microphone '{device_name}'. {MICROPHONE_HELP}")
                    })?
            }
            SampleFormat::U16 => {
                let sink = Arc::clone(&samples);
                let meter = Arc::clone(&visual_levels);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u16], _| {
                            append_mono(&sink, &meter, data, channels, |value| {
                                (value as f32 - 32_768.0) / 32_768.0
                            })
                        },
                        on_error,
                        None,
                    )
                    .with_context(|| {
                        format!("Could not open microphone '{device_name}'. {MICROPHONE_HELP}")
                    })?
            }
            format => return Err(anyhow!("unsupported microphone sample format: {format:?}")),
        };
        stream.play().with_context(|| {
            format!("Could not start microphone '{device_name}'. {MICROPHONE_HELP}")
        })?;

        tracing::info!(
            device = %device_name,
            sample_rate = source_sample_rate,
            channels,
            format = ?supported.sample_format(),
            "microphone capture started"
        );

        Ok(Self {
            _stream: stream,
            samples,
            visual_levels,
            source_sample_rate,
            error,
            device_name,
        })
    }

    pub fn snapshot(&self) -> Result<AudioSnapshot> {
        if let Some(message) = self.error.lock().ok().and_then(|error| error.clone()) {
            return Err(anyhow!(
                "Microphone stream failed: {message}. {MICROPHONE_HELP}"
            ));
        }
        let source = self
            .samples
            .lock()
            .map_err(|_| anyhow!("microphone sample buffer was poisoned"))?
            .clone();
        let duration_ms = source.len() as u64 * 1_000 / u64::from(self.source_sample_rate.max(1));
        Ok(AudioSnapshot {
            samples: resample_linear(&source, self.source_sample_rate, WHISPER_SAMPLE_RATE),
            duration_ms,
        })
    }

    pub fn visual_levels(&self) -> [f32; VISUALIZER_BARS] {
        self.visual_levels
            .lock()
            .map(|levels| *levels)
            .unwrap_or([0.0; VISUALIZER_BARS])
    }
}

fn append_mono<T: Copy>(
    sink: &Arc<Mutex<Vec<f32>>>,
    visual_levels: &Arc<Mutex<[f32; VISUALIZER_BARS]>>,
    data: &[T],
    channels: usize,
    convert: impl Fn(T) -> f32,
) {
    let mut chunk = Vec::with_capacity(data.len() / channels.max(1));
    let Ok(mut output) = sink.lock() else {
        return;
    };
    for frame in data.chunks_exact(channels.max(1)) {
        let sum = frame.iter().copied().map(&convert).sum::<f32>();
        let mono = sum / frame.len() as f32;
        output.push(mono);
        chunk.push(mono);
    }
    drop(output);
    update_visual_levels(visual_levels, &chunk);
}

fn update_visual_levels(visual_levels: &Arc<Mutex<[f32; VISUALIZER_BARS]>>, samples: &[f32]) {
    if samples.is_empty() {
        return;
    }
    let Ok(mut current) = visual_levels.lock() else {
        return;
    };
    let overall_rms =
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt();
    let overall_peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    // Preserve the sign of real microphone samples so the UI can draw a
    // continuous waveform above and below its centre line. The envelope
    // controls height, so louder speech remains visibly taller.
    let envelope = if overall_rms < 0.0015 {
        0.0
    } else {
        (overall_rms * 24.0).clamp(0.0, 1.0)
    };
    for (index, level) in current.iter_mut().enumerate() {
        let start = index * samples.len() / VISUALIZER_BARS;
        let end = ((index + 1) * samples.len() / VISUALIZER_BARS)
            .max(start + 1)
            .min(samples.len());
        let bucket = &samples[start.min(samples.len() - 1)..end];
        let signed_peak = bucket
            .iter()
            .copied()
            .max_by(|left, right| {
                left.abs()
                    .partial_cmp(&right.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0.0);
        let shape = if overall_peak > 0.000_01 {
            (signed_peak / overall_peak).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        *level = shape * envelope;
    }
}

fn resample_linear(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input.is_empty() || input_rate == 0 || output_rate == 0 {
        return Vec::new();
    }
    if input_rate == output_rate {
        return input.to_vec();
    }
    let output_len = input.len() * output_rate as usize / input_rate as usize;
    let ratio = input_rate as f64 / output_rate as f64;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let fraction = (position - left as f64) as f32;
            input[left] * (1.0 - fraction) + input[right] * fraction
        })
        .collect()
}

pub fn pcm16_wav(samples: &[f32]) -> Vec<u8> {
    let data_size = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&WHISPER_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(WHISPER_SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_whisper_wav_header() {
        let wav = pcm16_wav(&vec![0.0; WHISPER_SAMPLE_RATE as usize]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + WHISPER_SAMPLE_RATE as usize * 2);
    }

    #[test]
    fn resamples_to_sixteen_kilohertz() {
        let output = resample_linear(&vec![0.25; 48_000], 48_000, WHISPER_SAMPLE_RATE);
        assert_eq!(output.len(), 16_000);
        assert!(output
            .iter()
            .all(|sample| (*sample - 0.25).abs() < f32::EPSILON));
    }
}
