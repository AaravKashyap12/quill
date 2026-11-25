use crate::asr::AsrPass;
use crate::audio::WHISPER_SAMPLE_RATE;
use anyhow::{bail, Result};
use std::ffi::{c_char, CStr, CString};
use std::time::Instant;

extern "C" {
    fn quill_apple_speech_supported(language: *const c_char) -> bool;
    fn quill_apple_transcribe(
        language: *const c_char,
        samples: *const f32,
        count: u32,
        output: *mut *mut c_char,
    ) -> i32;
    fn quill_apple_free_text(text: *mut c_char);
}

pub fn supported(language: &str) -> bool {
    CString::new(language).is_ok_and(|tag| unsafe { quill_apple_speech_supported(tag.as_ptr()) })
}

pub fn transcribe(language: &str, samples: &[f32]) -> Result<AsrPass> {
    let start = Instant::now();
    let tag = CString::new(language)?;
    let mut pieces = Vec::new();
    let mut remaining = samples;
    while !remaining.is_empty() {
        let (chunk, rest) = remaining.split_at(chunk_length(remaining));
        let piece = transcribe_chunk(&tag, chunk)?;
        if !piece.is_empty() {
            pieces.push(piece);
        }
        remaining = rest;
    }
    let text = pieces.join(" ");
    Ok(AsrPass {
        words: super::synthetic_words(&text, samples.len()),
        text,
        latency_ms: start.elapsed().as_millis(),
        timings_synthetic: true,
    })
}

// Bound native buffer memory and recognition time for long dictations.
// Prefer a quiet 80 ms interval near the end of each minute.
fn chunk_length(samples: &[f32]) -> usize {
    let rate = WHISPER_SAMPLE_RATE as usize;
    let max = 60 * rate;
    if samples.len() <= max {
        return samples.len();
    }
    let start = 55 * rate;
    let frame = rate * 80 / 1000;
    let quietest = samples[start..max]
        .chunks_exact(frame)
        .enumerate()
        .map(|(i, window)| (i, window.iter().map(|v| v * v).sum::<f32>()))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("the split interval contains complete frames")
        .0;
    start + quietest * frame + frame / 2
}

fn transcribe_chunk(tag: &CStr, samples: &[f32]) -> Result<String> {
    let mut result = std::ptr::null_mut();
    // The synchronous bridge copies PCM and returns an owned UTF-8 string on success.
    let status = unsafe {
        quill_apple_transcribe(
            tag.as_ptr(),
            samples.as_ptr(),
            samples.len() as u32,
            &mut result,
        )
    };
    match status {
            0 => unsafe {
                let text = CStr::from_ptr(result).to_string_lossy().into_owned();
                quill_apple_free_text(result);
                Ok(text)
            },
            1 => bail!("Apple Speech is unavailable on this Mac for the selected language. Check the Dictation language in System Settings or choose another speech provider."),
            2 => bail!("Apple Speech is temporarily unavailable. Check Dictation in System Settings and try again."),
            4 => bail!("Apple Speech timed out. Try a shorter recording."),
            _ => bail!("Apple Speech could not finish transcribing. Check that Dictation is available for this language in System Settings."),
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the macOS English system voice and on-device recognition"]
    fn repeated_speech_survives_a_pause_without_duplicating_the_final_callback() {
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce).unwrap();
        let file =
            std::env::temp_dir().join(format!("quill-synthetic-{}.wav", u64::from_ne_bytes(nonce)));
        let status = std::process::Command::new("/usr/bin/say")
            .args([
                "-v",
                "Samantha",
                "--file-format=WAVE",
                "--data-format=LEI16@16000",
                "-o",
            ])
            .arg(&file)
            .arg("The blue bicycle is beside the garden gate.")
            .status()
            .unwrap();
        assert!(status.success());
        let phrase = hound::WavReader::open(&file)
            .unwrap()
            .into_samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect::<Vec<_>>();
        std::fs::remove_file(file).unwrap();
        let silence = vec![0.0; 2 * 16_000];
        let recording = [phrase.as_slice(), silence.as_slice(), phrase.as_slice()].concat();
        let text = transcribe("en", &recording).unwrap().text.to_lowercase();
        assert_eq!(text.matches("garden gate").count(), 2);
        assert!(transcribe("en", &silence).unwrap().text.is_empty());
    }

    #[test]
    #[ignore = "requires macOS English on-device recognition and a synthetic 16 kHz WAV fixture"]
    fn long_recording_preserves_beginning_middle_and_end() {
        // Generate a paragraph over one minute long with the three markers
        // below using macOS say and convert to mono PCM16 / 16 kHz.
        let path = std::env::var("QUILL_APPLE_SYNTHETIC_WAV").unwrap();
        let mut wav = hound::WavReader::open(path).unwrap();
        assert_eq!((wav.spec().channels, wav.spec().sample_rate), (1, 16_000));
        let samples = wav
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect::<Vec<_>>();
        assert!(samples.len() > 60 * 16_000);
        let pass = transcribe("en", &samples).unwrap();
        let text = pass.text.to_lowercase();
        for marker in ["purple bicycle", "little bird", "orange submarine"] {
            assert!(text.contains(marker), "missing synthetic marker: {marker}");
        }
    }
}
