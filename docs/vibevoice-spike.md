# VibeVoice-ASR spike — code-only pass

**Date:** 2026-08-03  
**Status:** No-go  
**Application code changed:** none

## Recommendation

Do not integrate VibeASR.cpp into Quill.

The premise for this evaluation was wrong. VibeASR's “hotwords” are one opaque
context string appended to its language-model prompt. That is architecturally
the same mechanism as whisper.cpp's initial prompt, not a stronger decoder-side
hotword facility. It therefore does not offer the distinct capability that was
supposed to justify a second ASR engine.

The Windows build also creates a permanent contributor cost. VibeASR.cpp
requires MinGW-w64 GCC/Clang and rejects MSVC, while Quill's Windows Tauri build
requires the Visual Studio Build Tools/MSVC toolchain. Every Windows contributor
would have to install, update, and troubleshoot both compiler ecosystems.

Reopen this decision only if upstream ships prebuilt Windows binaries **and** an
HTTP server. Those two changes would remove Quill's toolchain split and preserve
the existing local sidecar boundary.

This review is pinned to:

- `microsoft/VibeVoice` commit
  [`94da20d98b2fa7688e9cbfaf7692ddb4954f7600`](https://github.com/microsoft/VibeVoice/tree/94da20d98b2fa7688e9cbfaf7692ddb4954f7600)
- `microsoft/VibeASR.cpp` commit
  [`5cbce71c65911a7e10639ac13b6ab6929e4c8f9e`](https://github.com/microsoft/VibeASR.cpp/tree/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e)

The second repository is the relevant one: current VibeVoice delegates its
1.58 GB CPU runtime to `VibeASR.cpp` rather than implementing that runtime in
the Python/GPU repository.

## 1 — Short-utterance latency

**Blocked on free RAM. No number was measured.**

The machine has 15.3 GB installed but only 2.6 GB available during the spike.
Loading approximately 1.58 GB of model weights would leave too little margin
for the KV cache, VAE/LM working buffers, audio features, and the surrounding
processes. Closing user applications to manufacture benchmark headroom was
outside this read-only investigation.

Upstream reports RTF 0.78 at three threads on a 10.3-second clip using an
i7-13700 on Windows 11/MinGW, but that is a vendor result on different hardware
and cannot answer Quill's five-second wall-clock question. See the
[upstream performance table](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/README.md#L43-L69).

## 2 — Hotwords versus Quill's bias mechanism

### API shape

The backend does not expose a structured hotword list. It accepts one opaque
string:

- One-shot CLI: `--context <text>`.
- Persistent process: write `CONTEXT:<text>\n` to stdin and wait for
  `---ACK---`.
- The server stores that text as `context_info` and uses it for later audio
  requests without reloading either model.

See the
[`--context` argument and runtime update protocol](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/src/asr_server.cpp#L34-L73)
and the
[`CONTEXT:` command handling](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/src/asr_server.cpp#L402-L438).

The “hotwords” implementation is prompt context. `prompt_builder.h` inserts the
string verbatim into:

```text
This is a X.XX seconds audio, with extra info: {context}

Please transcribe it.
```

See the
[prompt construction](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/utils/prompt_builder.h#L155-L183).
There is no per-term weight, tokenizer-side boosting, trie, or dedicated
hotword decoder path.

### Format and limits

There is no backend hotword-count limit and no parsing of comma-separated or
newline-separated entries. The Gradio textbox says “one per line” and displays
up to five lines, but that is UI presentation rather than a model constraint.

The persistent server does have an implicit transport ceiling: its stdin line
buffer is 4,096 bytes. After `CONTEXT:`, the newline, and string termination,
roughly 4,086 bytes remain for context. Embedded newlines are unsafe on this
protocol because each line is treated as a new command or audio path. Quill's
current single-line, comma-separated 800-character bias string fits, including
worst-case four-byte UTF-8 characters.

### Can it consume Quill's Word replacements?

Yes. Quill can send the same single-line value it gives whisper.cpp:

```text
Tauri, whisper.cpp, qwen2.5
```

Only Word-entry replacements should be included, exactly as in the current
dictionary bias implementation. Snippets should remain excluded.

For privacy, Quill should start the sidecar without `--context` and send the
context over stdin afterward. Command-line arguments are visible to process
inspection tools; stdin is not. The server acknowledges context updates
without printing the value.

### Does it beat whisper.cpp's prompt?

**Blocked. No direct A/B was run.** Code inspection cannot answer comparative
recognition accuracy. Because the mechanism is itself generic prompt context,
there is no code-level basis for claiming superiority over Whisper's initial
prompt.

## 3 — Server or library?

The CPU repository builds two executables:

- `asr_infer`: one process per audio file.
- `asr_stream_server`: a persistent process that loads the models once.

Despite its name, `asr_stream_server` is not an HTTP server. Its protocol is:

1. stdout emits `---READY---` after model load;
2. stdin accepts `CONTEXT:...`, `FORMAT:...`, an audio-file path, or `EXIT`;
3. stdout emits generated text token by token;
4. `---END---` terminates one transcription.

The input is still a complete audio file; only output tokens stream. That is a
good fit for Scribe and not a replacement for Dictation's rolling audio path.
The protocol is documented directly in
[`asr_server.cpp`](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/src/asr_server.cpp#L1-L11).

There is no exported, stable VibeASR library target. The VAE sources are
compiled directly into both executables and linked with the repository's
llama.cpp fork. The separate Python VibeVoice repository has a vLLM HTTP path,
but that serves the much larger GPU model and is not the 1.58 GB CPU engine
being evaluated here.

Quill therefore does not need to build an HTTP wrapper. A Scribe-only adapter
can spawn `asr_stream_server`, pipe stdin/stdout, write the captured samples to
a temporary WAV, send its path, collect output until `---END---`, and preserve
the existing review window.

**Estimated integration effort:** 3–5 engineering days for a Windows-only
Scribe prototype after a reproducible binary package exists. This includes the
Rust process adapter, temporary-audio lifecycle, dynamic dictionary context,
timeouts/crash recovery, output parsing, and end-to-end tests. Cross-platform
build and installer validation would be additional work.

## 4 — Real footprint

**Blocked on free RAM. Peak RSS was not measured.**

Upstream claims 0.65 GB for the I8_S VAE plus 0.92 GB for the I2_S language
model, 1.58 GB total. This is an on-disk/model-weight claim, not process RSS;
it does not establish compatibility with Quill's 8 GB floor. See the
[upstream model-size table](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/README.md#L32-L40).

Neither the 1.58 GB disk figure nor actual peak RSS was independently verified
in this spike.

## 5 — Windows build cost

### Upstream requirements

- Recursive Git clone: the project pins a custom llama.cpp fork as a submodule.
- Python 3.9 or newer for model download/conversion and the optional demo.
- CMake 3.14 or newer.
- MinGW-w64 GCC/G++ or a functional Clang toolchain with C++11 support.
- MSVC is explicitly rejected by `src/CMakeLists.txt`.
- Approximately 2 GB for source plus quantized models, before build products.

Upstream recommends the `MinGW Makefiles` generator and requires the MinGW
`bin` directory on `PATH` at runtime so the executable can find its DLLs. It
also says not to use `setup_env.py` for a Windows build because that script's
Clang probe assumes a POSIX shell. See
[Notes for Windows](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/README.md#L157-L171)
and the
[compiler rejection](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/src/CMakeLists.txt#L1-L10).

The relevant build target is declared as
[`asr_stream_server`](https://github.com/microsoft/VibeASR.cpp/blob/5cbce71c65911a7e10639ac13b6ab6929e4c8f9e/CMakeLists.txt#L76-L101).

### This Windows machine

Present:

- Git 2.49
- Python 3.13 through the Windows `py` launcher

Missing from `PATH`:

- CMake
- GCC and G++
- Clang and Clang++
- `mingw32-make`

The project cannot currently be built here. Installing CMake and a MinGW-w64
distribution is required first.

### Tauri packaging assessment

Packaging as a Tauri sidecar is feasible because the desired persistent
executable already exists. The package would need:

- `asr_stream_server.exe` under Quill's target-triple sidecar naming scheme;
- its MinGW/llama/ggml runtime DLL dependencies alongside it;
- both quantized GGUF model files, approximately 1.58 GB by upstream's claim.

There are no GitHub release binaries and no Windows build workflow in the
repository, so Quill would own the reproducible build, dependency collection,
checksums, and installer validation.

**Build time and executable/DLL size were not measured.** Answering those
requires installing the missing toolchain and performing a build. Quoting an
estimate would violate this spike's measured-versus-inferred rule.

## Decision table

| Question | Result |
|---|---|
| 1 — Five-second latency | Blocked: insufficient free RAM for a trustworthy run |
| 2 — Hotwords | Same input fits; generic prompt context; comparison blocked |
| 3 — Integration | Persistent stdin/stdout executable, not HTTP; 3–5 day prototype |
| 4 — Peak memory | Blocked: no measured RSS |
| 5 — Windows build | Feasible with MinGW/CMake; required tools absent; no prebuilt package |
