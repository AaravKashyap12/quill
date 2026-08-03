#!/usr/bin/env bash
set -euo pipefail

backend="${1:-metal}"
revision="${WHISPER_REVISION:-master}"
workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="$workspace/third_party/whisper.cpp"
build_dir="$source_dir/build-quill"
destination="$workspace/apps/desktop/src-tauri/binaries"

if [[ ! -d "$source_dir/.git" ]]; then
  git clone https://github.com/ggml-org/whisper.cpp.git "$source_dir"
fi

git -C "$source_dir" fetch --tags origin
git -C "$source_dir" checkout "$revision"

args=(
  -S "$source_dir"
  -B "$build_dir"
  -DWHISPER_BUILD_EXAMPLES=ON
  -DWHISPER_SDL2=ON
  -DCMAKE_BUILD_TYPE=Release
)

if [[ "$backend" == "metal" ]]; then
  args+=(-DGGML_METAL=ON)
fi

cmake "${args[@]}"
cmake --build "$build_dir" --config Release --target whisper-stream
mkdir -p "$destination"
cp "$build_dir/bin/whisper-stream" "$destination/whisper-stream-aarch64-apple-darwin"
echo "Built whisper-stream ($backend) at $destination"
