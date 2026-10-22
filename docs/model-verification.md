# Speech-model verification

Quill pins every downloadable speech model in `downloads.rs` to a Hugging Face
repository revision, exact byte count, and SHA-256 digest. These are the Git LFS
object digests, not the Git blob IDs of the pointer files. The metadata below
was fetched on 2026-10-22; no model weights were downloaded to obtain it.

## Sources

- [ggerganov/whisper.cpp LFS metadata](https://huggingface.co/api/models/ggerganov/whisper.cpp/revision/5359861c739e955e79d9a303bcbc70fb988958b1?blobs=true), revision `5359861c739e955e79d9a303bcbc70fb988958b1`.
- [distil-whisper/distil-large-v3-ggml LFS metadata](https://huggingface.co/api/models/distil-whisper/distil-large-v3-ggml/revision/0d78dd96ed9fc152325f63b53788fec3b43de031?blobs=true), revision `0d78dd96ed9fc152325f63b53788fec3b43de031`.

## Pinned artifacts

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `ggml-base.bin` | 147951465 | `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe` |
| `ggml-base.en.bin` | 147964211 | `a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002` |
| `ggml-large-v3-turbo.bin` | 1624555275 | `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69` |
| `ggml-medium.bin` | 1533763059 | `6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208` |
| `ggml-medium.en.bin` | 1533774781 | `cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356` |
| `ggml-small.bin` | 487601967 | `1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b` |
| `ggml-small.en.bin` | 487614201 | `c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d` |
| `ggml-tiny.bin` | 77691713 | `be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21` |
| `ggml-tiny.en.bin` | 77704715 | `921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f` |
| `ggml-distil-large-v3.bin` | 1519521155 | `2883a11b90fb10ed592d826edeaee7d2929bf1ab985109fe9e1e7b4d2b69a298` |

## Enforcement and limits

- Downloads use commit-specific URLs and stream SHA-256 while writing a `.part`
  file. Only a matching digest can be renamed into the installed `.bin` path.
  A checksum mismatch removes the partial file. Interrupted transport can leave
  a `.part` file; it is not an installed model and is replaced on retry.
- Before each local speech-engine startup, Quill checks the selected installed
  file's size and SHA-256 on a blocking worker. This includes legacy downloads
  and both bundled model layouts. Invalid files are refused with a remove-and-
  download-again error; they are not silently deleted. Unknown IDs are refused.
- The model selector currently discovers filenames. An installed-looking file
  can therefore still fail verification at engine startup. File presence is not
  an integrity check.
- Hashing reads the full model before startup and adds disk I/O to a cold local
  engine start. The running engine does not rehash on every transcription pass.
- This pins artifact identity, not model accuracy or absence of malicious model
  behavior. It assumes the local model directory is not concurrently modified
  by another process between verification and sidecar loading.
- Ollama cleanup-model pulls and the optional CUDA runtime use separate paths.
  These speech-model pins do not establish their provenance. CUDA archives are
  checked against the release's checksum file, not this pinned registry.

To update a pin, fetch the upstream metadata for a specific revision, compare
its LFS `sha256` and `size`, update the registry and revision together, and run
the download/installed-model regression tests. Do not copy a digest from an
untrusted mirror or substitute the metadata response's `blobId`.
