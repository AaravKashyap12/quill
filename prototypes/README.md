# Scribe trailing-buffer prototype

This historical, dependency-free prototype explores two streaming contracts
without audio capture or hotkey code. The desktop app now inserts the completed
utterance after release; these tests do not prove streaming insertion in the app:

- `LocalAgreement` commits a word only after two consecutive ASR passes share
  the same prefix.
- `TrailingScribeBuffer` keeps recent words editable, cleans the rolling
  transcript, and commits only text that is both old enough and stable across
  cleanup passes.

Historical note: this prototype enforces a strict word-provenance gate that
rejects any output containing lexical tokens absent from the source. Production
`cleanup.rs` has since moved to a prompt-based safety model (fix punctuation,
mishearings, and self-corrections but preserve every specific fact and never
invent new content), action-specific output budgets and deterministic fallback
guards, followed by mandatory user review before insertion. The strict gate
was removed because it blocked legitimate rewrites like `hei` → `Hey`.

```powershell
python prototypes/test_scribe_buffer.py
python prototypes/scribe_buffer.py
```
