# Scribe trailing-buffer prototype

This dependency-free prototype validates the two streaming contracts before
audio capture or hotkey code is involved:

- `LocalAgreement` commits a word only after two consecutive ASR passes share
  the same prefix.
- `TrailingScribeBuffer` keeps recent words editable, cleans the rolling
  transcript, and commits only text that is both old enough and stable across
  cleanup passes.

Historical note: this prototype enforces a strict word-provenance gate that
rejects any output containing lexical tokens absent from the source. Production
`cleanup.rs` has since moved to a prompt-based safety model (fix punctuation,
mishearings, and self-corrections but preserve every specific fact and never
invent new content) plus a lightweight sanity guard on empty or 3×-ballooned
outputs. The strict gate was removed because it blocked legitimate rewrites
like `hei` → `Hey`.

```powershell
python prototypes/test_scribe_buffer.py
python prototypes/scribe_buffer.py
```
