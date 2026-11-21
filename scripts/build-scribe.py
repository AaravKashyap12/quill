"""Build Quill's pinned, offline-only llama.cpp sidecar (macOS/Windows)."""
import os
from pathlib import Path
import platform
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REVISION = "cd959bae99ff4bed22bef2604f45241c09bbb8c8"  # llama.cpp v0.9.0
SOURCE = ROOT / "third_party/llama.cpp"


def run(*args):
    subprocess.run([str(arg) for arg in args], check=True)


def main():
    system = platform.system()
    if system not in ("Darwin", "Windows"):
        raise SystemExit("Scribe bundles are supported on macOS and Windows")
    if not shutil.which("cmake"):
        raise SystemExit("CMake is required to build the Scribe engine")
    if not SOURCE.exists():
        run("git", "clone", "--depth", "1", "--branch", "v0.9.0",
            "https://github.com/ggml-org/llama.cpp.git", SOURCE)
    head = subprocess.check_output(["git", "-C", str(SOURCE), "rev-parse", "HEAD"], text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(SOURCE), "status", "--porcelain"], text=True).strip()
    if head != REVISION or dirty:
        raise SystemExit("llama.cpp must be clean at the pinned revision; preserve local edits")
    options = ["-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF",
               "-DGGML_NATIVE=OFF", "-DGGML_OPENMP=OFF", "-DLLAMA_BUILD_TESTS=OFF", "-DLLAMA_BUILD_EXAMPLES=OFF",
               "-DLLAMA_BUILD_APP=OFF", "-DLLAMA_BUILD_SERVER=ON", "-DLLAMA_BUILD_UI=OFF",
               "-DLLAMA_USE_PREBUILT_UI=OFF", "-DLLAMA_OPENSSL=OFF"]
    x86_options = ["-DGGML_SSE42=ON", "-DGGML_AVX=ON", "-DGGML_AVX2=ON",
                   "-DGGML_F16C=ON", "-DGGML_FMA=ON"]
    destination = ROOT / "apps/desktop/src-tauri/binaries"
    destination.mkdir(parents=True, exist_ok=True)
    architectures = ["arm64", "x86_64"] if system == "Darwin" else ["x86_64"]
    built = []
    for arch in architectures:
        build = SOURCE / f"build-quill-{system.lower()}-{arch}"
        arch_options = x86_options if arch == "x86_64" else []
        if system == "Darwin":
            arch_options += [f"-DCMAKE_OSX_ARCHITECTURES={arch}", "-DCMAKE_OSX_DEPLOYMENT_TARGET=12.0",
                             "-DGGML_METAL=ON", "-DGGML_METAL_EMBED_LIBRARY=ON"]
            binary = build / "bin/llama-server"
        else:
            arch_options += ["-A", "x64", "-DGGML_CUDA=OFF", "-DGGML_VULKAN=OFF",
                             "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded"]
            binary = build / "bin/Release/llama-server.exe"
        run("cmake", "-S", SOURCE, "-B", build, *options, *arch_options)
        run("cmake", "--build", build, "--config", "Release", "--target", "llama-server",
            "--parallel", str(min(os.cpu_count() or 2, 8)))
        built.append(binary)
    if system == "Darwin":
        binary = destination / "llama-server-universal-apple-darwin"
        run("lipo", "-create", *built, "-output", binary)
        for arch in architectures:
            run("lipo", binary, "-verify_arch", arch)
        triples = ["aarch64-apple-darwin", "x86_64-apple-darwin"]
    else:
        binary = built[0]
        triples = ["x86_64-pc-windows-msvc"]
    for triple in triples:
        output = destination / (f"llama-server-{triple}" + (".exe" if system == "Windows" else ""))
        shutil.copy2(binary, output)
        print(f"Built pinned Scribe engine: {output}")


if __name__ == "__main__":
    main()
