#!/usr/bin/env bash
set -euo pipefail

backend="${1:-metal}"
if [[ "$backend" != "metal" ]]; then
  echo "Unsupported macOS whisper.cpp backend: $backend (expected: metal)" >&2
  exit 1
fi

# Keep this revision aligned with resources/whisper/manifest.json and the
# Windows runtime. A sidecar built from a different revision is not a valid
# release input.
readonly whisper_version="v1.9.1"
readonly whisper_revision="f049fff95a089aa9969deb009cdd4892b3e74916"
readonly target_triple="universal-apple-darwin"

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="$workspace/third_party/whisper.cpp"
build_dir="$source_dir/build-quill-macos-universal"
destination="$workspace/apps/desktop/src-tauri/binaries"
sidecar="$destination/whisper-server-$target_triple"

if [[ ! -d "$source_dir/.git" ]]; then
  git clone https://github.com/ggml-org/whisper.cpp.git "$source_dir"
fi

git -C "$source_dir" fetch --tags origin
git -C "$source_dir" checkout --detach "$whisper_revision"

cmake \
  -S "$source_dir" \
  -B "$build_dir" \
  -DCMAKE_BUILD_TYPE=Release \
  '-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64' \
  -DBUILD_SHARED_LIBS=OFF \
  -DGGML_METAL=ON \
  -DGGML_METAL_EMBED_LIBRARY=ON \
  -DWHISPER_BUILD_TESTS=OFF \
  -DWHISPER_BUILD_EXAMPLES=ON \
  -DWHISPER_BUILD_SERVER=ON \
  -DWHISPER_SDL2=OFF

cmake --build "$build_dir" --config Release --target whisper-server --parallel

built_server="$build_dir/bin/whisper-server"
if [[ ! -x "$built_server" ]]; then
  echo "whisper-server was not produced as an executable at $built_server" >&2
  exit 1
fi
if ! lipo -verify_arch arm64 x86_64 "$built_server"; then
  echo "whisper-server is not a universal arm64+x86_64 binary: $built_server" >&2
  exit 1
fi

mkdir -p "$destination"
cp "$built_server" "$sidecar"
chmod 755 "$sidecar"

# Native cargo test/clippy jobs still resolve externalBin using their host
# triple. Each copy remains a universal executable; the names only satisfy
# Tauri's target-suffixed sidecar lookup before the universal release build.
for native_triple in aarch64-apple-darwin x86_64-apple-darwin; do
  native_sidecar="$destination/whisper-server-$native_triple"
  cp "$built_server" "$native_sidecar"
  chmod 755 "$native_sidecar"
done

echo "Built pinned whisper.cpp $whisper_version ($whisper_revision) Metal sidecar: $sidecar"
