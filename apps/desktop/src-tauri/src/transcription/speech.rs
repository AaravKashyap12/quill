use webrtc_vad::{SampleRate, Vad, VadMode};

const FRAME_SAMPLES: usize = 320; // 20 ms at Quill's 16 kHz capture rate.
const MIN_VOICED_FRAMES: usize = 3;
const PADDING_FRAMES: usize = 10;
const MIN_ACTIVE_RMS: f32 = 0.0001;
const MAX_CREST_FACTOR: f32 = 10.0;
const MIN_VOICE_RATIO: f32 = 0.08;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpeechWindow {
    pub start_sample: usize,
    pub end_sample: usize,
    pub(crate) activity: Vec<bool>,
}

impl SpeechWindow {
    pub(crate) fn samples<'a>(&self, samples: &'a [f32]) -> &'a [f32] {
        &samples[self.start_sample..self.end_sample]
    }

    pub(crate) fn start_ms(&self) -> u64 {
        self.start_sample as u64 * 1_000 / crate::audio::WHISPER_SAMPLE_RATE as u64
    }

    /// Confirm that a provider segment overlaps sustained acoustic speech,
    /// rather than a handful of impulsive frames spread across silence.
    pub(crate) fn supports_interval(&self, start_seconds: f64, end_seconds: f64) -> bool {
        if !start_seconds.is_finite()
            || !end_seconds.is_finite()
            || end_seconds <= start_seconds
            || self.activity.is_empty()
        {
            return false;
        }
        let frames_per_second = crate::audio::WHISPER_SAMPLE_RATE as f64 / FRAME_SAMPLES as f64;
        let start = (start_seconds.max(0.0) * frames_per_second).floor() as usize;
        let end = (end_seconds.max(0.0) * frames_per_second).ceil() as usize;
        let start = start.min(self.activity.len());
        let end = end.min(self.activity.len()).max(start);
        let interval = &self.activity[start..end];
        if interval.is_empty() {
            return false;
        }
        let voiced = interval.iter().filter(|decision| **decision).count();
        voiced >= 2 && voiced as f32 / interval.len() as f32 >= MIN_VOICE_RATIO
    }
}

fn frame_is_speech_evidence(frame: &[f32], vad_decision: bool) -> bool {
    if !vad_decision || frame.is_empty() {
        return false;
    }
    let rms = (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt();
    if rms < MIN_ACTIVE_RMS {
        return false;
    }
    let peak = frame
        .iter()
        .fold(0.0_f32, |current, sample| current.max(sample.abs()));
    peak / rms.max(f32::EPSILON) <= MAX_CREST_FACTOR
}

fn window_from_activity(activity: &[bool], sample_count: usize) -> Option<SpeechWindow> {
    if activity.iter().filter(|decision| **decision).count() < MIN_VOICED_FRAMES {
        return None;
    }
    let first = activity.iter().position(|decision| *decision)?;
    let last = activity.iter().rposition(|decision| *decision)?;
    let start_frame = first.saturating_sub(PADDING_FRAMES);
    let end_frame = (last + 1 + PADDING_FRAMES).min(activity.len());
    let window_activity = activity[start_frame..end_frame].to_vec();
    Some(SpeechWindow {
        start_sample: start_frame * FRAME_SAMPLES,
        end_sample: (end_frame * FRAME_SAMPLES).min(sample_count),
        activity: window_activity,
    })
}

/// Find the speech-bearing portion of a completed recording with WebRTC VAD.
///
/// Requiring several voiced frames prevents a click or handling noise from
/// turning an otherwise silent recording into a cloud request. Padding keeps
/// soft consonants at each boundary. Internal pauses are deliberately retained
/// so the recognizer still receives the utterance's natural context.
pub(crate) fn detect_speech_window(samples: &[f32]) -> Option<SpeechWindow> {
    if samples.len() < FRAME_SAMPLES {
        return None;
    }

    let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Quality);
    let mut voiced = Vec::with_capacity(samples.len() / FRAME_SAMPLES);
    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        let pcm = frame
            .iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
            .collect::<Vec<_>>();
        // WebRTC intentionally keeps a short speech "hangover" after a
        // positive frame. Requiring energy in the same frame prevents one
        // keyboard/cable impulse from turning those trailing zero frames into
        // a synthetic utterance.
        voiced.push(frame_is_speech_evidence(
            frame,
            vad.is_voice_segment(&pcm).unwrap_or(false),
        ));
    }
    window_from_activity(&voiced, samples.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(frames: usize, active: &[usize]) -> Vec<bool> {
        (0..frames).map(|index| active.contains(&index)).collect()
    }

    #[test]
    fn silence_is_not_speech() {
        assert_eq!(detect_speech_window(&vec![0.0; 16_000]), None);
    }

    #[test]
    fn isolated_click_is_not_enough_to_upload() {
        let mut samples = vec![0.0; 16_000];
        samples[8_000] = 0.9;
        assert_eq!(detect_speech_window(&samples), None);
    }

    #[test]
    fn quiet_sustained_audio_remains_speech_evidence() {
        let frame = (0..FRAME_SAMPLES)
            .map(|index| {
                let phase = index as f32 * std::f32::consts::TAU * 180.0
                    / crate::audio::WHISPER_SAMPLE_RATE as f32;
                phase.sin() * 0.0005
            })
            .collect::<Vec<_>>();
        assert!(frame_is_speech_evidence(&frame, true));
    }

    #[test]
    fn sparse_repeated_impulses_do_not_support_a_provider_segment() {
        let decisions = activity(100, &[5, 35, 70, 90]);
        let window = window_from_activity(&decisions, decisions.len() * FRAME_SAMPLES)
            .expect("the provider may still be queried so its confidence can be checked");
        assert!(!window.supports_interval(0.0, 2.0));
    }

    #[test]
    fn genuine_speech_with_a_long_internal_pause_remains_an_utterance() {
        let active = (5..15).chain(65..75).collect::<Vec<_>>();
        let decisions = activity(80, &active);
        let window = window_from_activity(&decisions, decisions.len() * FRAME_SAMPLES)
            .expect("speech on both sides of a pause must survive");
        assert!(window.supports_interval(0.0, 1.6));
    }

    #[test]
    fn a_very_long_pause_does_not_invalidate_real_speech() {
        let active = (5..10).chain(190..195).collect::<Vec<_>>();
        let decisions = activity(200, &active);
        assert!(window_from_activity(&decisions, decisions.len() * FRAME_SAMPLES).is_some());
    }

    #[test]
    fn speech_window_padding_and_slicing_are_bounded() {
        let decisions = [false, false, true, true, true, false];
        let first = decisions.iter().position(|decision| *decision).unwrap();
        let last = decisions.iter().rposition(|decision| *decision).unwrap();
        let start_frame = first.saturating_sub(PADDING_FRAMES);
        let end_frame = (last + 1 + PADDING_FRAMES).min(decisions.len());
        let window = SpeechWindow {
            start_sample: start_frame * FRAME_SAMPLES,
            end_sample: end_frame * FRAME_SAMPLES,
            activity: decisions[start_frame..end_frame].to_vec(),
        };
        let samples = vec![0.0; decisions.len() * FRAME_SAMPLES];
        assert_eq!(window.samples(&samples).len(), samples.len());
    }
}
