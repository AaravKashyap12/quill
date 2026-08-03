use crate::model::DictionaryEntry;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
struct Match<'a> {
    start: usize,
    end: usize,
    replacement: &'a str,
}

/// Apply the user's literal dictionary to a transcript.
///
/// Matches are selected longest-first against the original transcript. This
/// means a replacement is never scanned again and shorter entries cannot
/// consume part of a longer entry. Entry contents are intentionally never
/// logged: names, addresses, and snippets are private user text.
pub fn apply(transcript: &str, entries: &[DictionaryEntry]) -> String {
    if transcript.is_empty() || entries.is_empty() {
        return transcript.to_owned();
    }

    let mut ordered: Vec<&DictionaryEntry> = entries
        .iter()
        .filter(|entry| !entry.spoken.is_empty())
        .collect();
    ordered.sort_by(|left, right| {
        right
            .spoken
            .chars()
            .count()
            .cmp(&left.spoken.chars().count())
    });

    let mut selected = Vec::<Match<'_>>::new();
    for entry in ordered {
        for (start, end) in literal_matches(transcript, &entry.spoken) {
            if selected
                .iter()
                .any(|existing| start < existing.end && end > existing.start)
            {
                continue;
            }
            selected.push(Match {
                start,
                end,
                replacement: &entry.replacement,
            });
        }
    }

    if selected.is_empty() {
        return transcript.to_owned();
    }
    selected.sort_by_key(|matched| matched.start);

    let mut output = String::with_capacity(transcript.len());
    let mut cursor = 0;
    for matched in selected {
        output.push_str(&transcript[cursor..matched.start]);
        if starts_sentence(transcript, matched.start) {
            output.push_str(&capitalize_first(matched.replacement));
        } else {
            output.push_str(matched.replacement);
        }
        cursor = matched.end;
    }
    output.push_str(&transcript[cursor..]);
    output
}

fn literal_matches(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    let needle_chars = needle.chars().count();
    if needle_chars == 0 {
        return Vec::new();
    }
    let folded_needle: String = needle.chars().flat_map(char::to_lowercase).collect();
    let boundaries: Vec<usize> = haystack
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(haystack.len()))
        .collect();
    if boundaries.len() <= needle_chars {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for start_index in 0..=(boundaries.len() - needle_chars - 1) {
        let end_index = start_index + needle_chars;
        let start = boundaries[start_index];
        let end = boundaries[end_index];
        let candidate = &haystack[start..end];
        let folded_candidate: String = candidate.chars().flat_map(char::to_lowercase).collect();
        if folded_candidate != folded_needle {
            continue;
        }
        if is_word_boundary(haystack, start, end, needle) {
            matches.push((start, end));
        }
    }
    matches
}

fn is_word_boundary(haystack: &str, start: usize, end: usize, needle: &str) -> bool {
    let begins_with_word = needle.chars().next().is_some_and(is_word_character);
    let ends_with_word = needle.chars().next_back().is_some_and(is_word_character);
    let is_boundary = |candidate| {
        candidate == haystack.len()
            || haystack
                .split_word_bound_indices()
                .any(|(index, _)| index == candidate)
    };

    (!begins_with_word || is_boundary(start)) && (!ends_with_word || is_boundary(end))
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn starts_sentence(transcript: &str, start: usize) -> bool {
    let mut before = transcript[..start].chars().rev();
    loop {
        match before.next() {
            None => return true,
            Some(character) if character.is_whitespace() => {}
            Some('"' | '\'' | ')' | ']' | '}' | '\u{2019}' | '\u{201d}') => {}
            Some('.' | '!' | '?') => return true,
            Some(_) => return false,
        }
    }
}

fn capitalize_first(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DictionaryKind;

    fn entry(id: &str, spoken: &str, replacement: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: id.into(),
            spoken: spoken.into(),
            replacement: replacement.into(),
            kind: DictionaryKind::Word,
        }
    }

    #[test]
    fn longest_match_wins_even_when_shorter_entry_comes_first() {
        let entries = [
            entry("short", "my email", "wrong@example.com"),
            entry("long", "my email address", "aarav@example.com"),
        ];
        assert_eq!(
            apply("send it to my email address", &entries),
            "send it to aarav@example.com"
        );
    }

    #[test]
    fn rejects_matches_inside_unicode_words() {
        let entries = [entry("cat", "cat", "dog"), entry("dev", "न", "X")];
        assert_eq!(apply("catalogue", &entries), "catalogue");
        assert_eq!(apply("नमस्ते", &entries), "नमस्ते");
    }

    #[test]
    fn rejects_a_boundary_inside_a_combining_character_sequence() {
        let entries = [entry("dev", "न", "X")];
        assert_eq!(apply("न\u{093c}", &entries), "न\u{093c}");
    }

    #[test]
    fn capitalizes_a_replacement_at_the_start_of_a_sentence() {
        let entries = [entry("tauri", "tory", "tauri")];
        assert_eq!(
            apply("tory is local. tory is fast", &entries),
            "Tauri is local. Tauri is fast"
        );
    }

    #[test]
    fn matches_before_trailing_punctuation_without_consuming_it() {
        let entries = [entry("whisper", "whisper dot cpp", "whisper.cpp")];
        assert_eq!(apply("use whisper dot cpp.", &entries), "use whisper.cpp.");
    }

    #[test]
    fn empty_dictionary_is_a_no_op() {
        assert_eq!(apply("Leave me alone.", &[]), "Leave me alone.");
    }

    #[test]
    fn overlapping_entries_do_not_double_apply_or_rescan_replacements() {
        let entries = [
            entry("long", "new york city", "NYC"),
            entry("middle", "york city", "York"),
            entry("replacement", "NYC", "should not appear"),
        ];
        assert_eq!(apply("new york city", &entries), "NYC");
    }

    #[test]
    fn matching_is_case_insensitive() {
        let entries = [entry("tauri", "tory", "Tauri")];
        assert_eq!(apply("TORY and tOrY", &entries), "Tauri and Tauri");
    }

    #[test]
    fn applies_the_documented_word_and_snippet_examples() {
        let entries = [
            entry("tauri", "tory", "Tauri"),
            entry("whisper", "whisper dot cpp", "whisper.cpp"),
            DictionaryEntry {
                id: "email".into(),
                spoken: "my email".into(),
                replacement: "aarav@example.com".into(),
                kind: DictionaryKind::Snippet,
            },
        ];
        assert_eq!(
            apply("the tory sidecar spawns whisper dot cpp", &entries),
            "the Tauri sidecar spawns whisper.cpp"
        );
        assert_eq!(
            apply("send the invoice to my email", &entries),
            "send the invoice to aarav@example.com"
        );
    }
}
