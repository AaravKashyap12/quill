use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Streaming commit policy (LocalAgreement-2).
///
/// Each pass re-transcribes the whole cumulative buffer, so successive
/// hypotheses share a growing prefix. A word is committed once two
/// consecutive hypotheses agree on it. Crucially, the boundary between
/// "already typed" and "new" is anchored to the *text of the words we
/// actually emitted*, not a raw index — so when whisper re-segments the
/// buffer on a later pass (merging or splitting a token, shifting timings),
/// we neither retype a committed word nor skip the next one.
#[derive(Debug, Default)]
pub struct LocalAgreement {
    previous: Vec<TimedWord>,
    committed: Vec<TimedWord>,
}

impl LocalAgreement {
    pub fn update(&mut self, hypothesis: Vec<TimedWord>) -> Vec<TimedWord> {
        // Words agreed by the last two hypotheses: their shared prefix.
        let agreed = self
            .previous
            .iter()
            .zip(hypothesis.iter())
            .take_while(|(left, right)| same_word(&left.text, &right.text))
            .count();

        // How much of this hypothesis we've already emitted, matched by text.
        let already = self.committed_prefix_len(&hypothesis);
        // Only commit words that are both agreed and not yet emitted. Clamp to
        // the hypothesis length so a shorter re-transcription can never panic.
        let stable_until = agreed.max(already).min(hypothesis.len());
        let fresh = hypothesis[already..stable_until].to_vec();

        self.committed.extend(fresh.iter().cloned());
        self.previous = hypothesis;
        fresh
    }

    pub fn flush(&mut self, hypothesis: Vec<TimedWord>) -> Vec<TimedWord> {
        let already = self.committed_prefix_len(&hypothesis);
        let remaining = hypothesis[already..].to_vec();
        self.committed.extend(remaining.iter().cloned());
        self.previous = hypothesis;
        remaining
    }

    /// Number of leading `hypothesis` words we have already committed, matched
    /// by normalized text. Whenever the hypothesis diverges from our committed
    /// prefix at ANY position (the model revised an already-typed word), fall
    /// back to committed.len() so we skip past every word we typed instead of
    /// re-emitting the tail. The earlier "matched == 0" bound missed the
    /// common case where the first word still agrees but a later one differs
    /// — e.g. committed "a blue fox", hypothesis "a red fox" — and would
    /// happily re-emit "red fox" on the next agreeing pass.
    fn committed_prefix_len(&self, hypothesis: &[TimedWord]) -> usize {
        let matched = self
            .committed
            .iter()
            .zip(hypothesis.iter())
            .take_while(|(committed, candidate)| same_word(&committed.text, &candidate.text))
            .count();
        if matched < self.committed.len() && !self.committed.is_empty() {
            self.committed.len().min(hypothesis.len())
        } else {
            matched
        }
    }
}

/// Compare two words ignoring case and surrounding punctuation, so
/// "Fox", "fox" and "fox." are treated as the same committed word.
fn same_word(left: &str, right: &str) -> bool {
    normalize(left) == normalize(right)
}

fn normalize(word: &str) -> String {
    word.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<TimedWord> {
        text.split_whitespace()
            .enumerate()
            .map(|(index, text)| TimedWord {
                text: text.into(),
                start_ms: index as u64 * 300,
                end_ms: (index as u64 + 1) * 300,
            })
            .collect()
    }

    fn texts(committed: &[TimedWord]) -> Vec<&str> {
        committed.iter().map(|word| word.text.as_str()).collect()
    }

    #[test]
    fn commits_only_shared_prefix() {
        let mut agreement = LocalAgreement::default();
        assert!(agreement.update(words("a red fox")).is_empty());
        assert_eq!(agreement.update(words("a blue fox")), words("a"));
        let committed = agreement.update(words("a blue fox runs"));
        assert_eq!(texts(&committed), vec!["blue", "fox"]);
    }

    #[test]
    fn does_not_duplicate_when_hypothesis_is_resegmented() {
        // committed "the quick", then whisper re-segments punctuation on the
        // next pass ("quick," vs "quick"). The comma-anchored match must not
        // retype "quick" nor skip "brown".
        let mut agreement = LocalAgreement::default();
        agreement.update(words("the quick"));
        assert_eq!(
            texts(&agreement.update(words("the quick"))),
            vec!["the", "quick"]
        );
        // Whisper now attaches a comma to the already-committed "quick".
        assert!(agreement.update(words("the quick, brown")).is_empty());
        let committed = agreement.update(words("the quick, brown"));
        assert_eq!(texts(&committed), vec!["brown"]);
    }

    #[test]
    fn flush_emits_only_the_uncommitted_tail() {
        let mut agreement = LocalAgreement::default();
        agreement.update(words("hello there"));
        agreement.update(words("hello there"));
        let tail = agreement.flush(words("hello there friend"));
        assert_eq!(texts(&tail), vec!["friend"]);
    }

    #[test]
    fn does_not_reemit_when_a_later_committed_word_is_revised() {
        // Reviewer scenario: we've already emitted "a blue fox". Whisper now
        // revises "blue" → "red" and returns "a red fox" twice. Because the
        // first hypothesis word ("a") still agrees, matched=1, but the
        // remaining committed words diverge. The buggy version returned
        // matched=1 and then, on the second agreeing pass, emitted "red fox"
        // a second time. The fix: fall back to committed.len() whenever any
        // committed word diverges, so we never re-emit the tail.
        let mut agreement = LocalAgreement {
            committed: words("a blue fox"),
            previous: words("a blue fox"),
        };
        assert!(
            agreement.update(words("a red fox")).is_empty(),
            "divergent hypothesis must not re-emit committed words"
        );
        assert!(
            agreement.update(words("a red fox")).is_empty(),
            "second agreeing but still-divergent pass must not re-emit committed words either"
        );
    }

    #[test]
    fn shorter_hypothesis_does_not_panic() {
        // A later pass that returns fewer words than we've committed must not
        // index out of bounds.
        let mut agreement = LocalAgreement::default();
        agreement.update(words("one two three"));
        agreement.update(words("one two three"));
        assert!(agreement.update(words("one two")).is_empty());
        assert!(agreement.flush(words("one")).is_empty());
    }
}
