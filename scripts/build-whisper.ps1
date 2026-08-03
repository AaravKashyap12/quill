param(
  [string]$CacheDirectory = (Join-Path $env:TEMP "quill-whisper"),
  [switch]$ForceDownload
)

$ErrorActionPreference = "Stop"
$workspace = Resolve-Path (Join-Path $PSScriptRoot "..")
$runtimeDestination = Join-Path $workspace "apps\desktop\src-tauri\resources\whisper\windows-x64-cuda"

$artifacts = @{
  Whisper = @{
    Url = "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.1/whisper-cublas-11.8.0-bin-x64.zip"
    File = "whisper-cublas-11.8.0-bin-x64.zip"
    Sha256 = "aecdce0e4d4bb758a7c72a31f3f9f19a7b6d861405fd2da743cd86398633c963"
  }
  Cublas = @{
    Url = "https://developer.download.nvidia.com/compute/cublas/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-11.8.1.74-archive.zip"
    File = "libcublas-windows-x86_64-11.8.1.74-archive.zip"
    Sha256 = "d0a110abef0c2d302d90b141772cb39ef1a94c4d9a7215b0c4b6bd869fdae644"
  }
}

function Get-VerifiedArtifact {
  param([hashtable]$Artifact)

  New-Item -ItemType Directory -Force -Path $CacheDirectory | Out-Null
  $path = Join-Path $CacheDirectory $Artifact.File
  if ($ForceDownload -or -not (Test-Path -LiteralPath $path)) {
    Invoke-WebRequest -Uri $Artifact.Url -OutFile $path
  }
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
  if ($actual -ne $Artifact.Sha256) {
    throw "SHA-256 mismatch for $($Artifact.File): expected $($Artifact.Sha256), got $actual"
  }
  return $path
}

$whisperArchive = Get-VerifiedArtifact $artifacts.Whisper
$cublasArchive = Get-VerifiedArtifact $artifacts.Cublas
$staging = Join-Path $CacheDirectory ("staging-" + [guid]::NewGuid().ToString("N"))
$whisperStaging = Join-Path $staging "whisper"
$cublasStaging = Join-Path $staging "cublas"

New-Item -ItemType Directory -Force -Path $whisperStaging, $cublasStaging, $runtimeDestination | Out-Null
Expand-Archive -LiteralPath $whisperArchive -DestinationPath $whisperStaging
Expand-Archive -LiteralPath $cublasArchive -DestinationPath $cublasStaging

$whisperRelease = Join-Path $whisperStaging "Release"
$runtimeFiles = @(
  "whisper-server.exe",
  "whisper-cli.exe",
  "whisper.dll",
  "ggml.dll",
  "ggml-base.dll",
  "ggml-cuda.dll",
  "cudart32_110.dll",
  "cudart64_110.dll",
  "cuinj64_118.dll",
  "nvrtc-builtins64_118.dll",
  "nvrtc64_112_0.dll"
)
$runtimeFiles += Get-ChildItem -LiteralPath $whisperRelease -Filter "ggml-cpu-*.dll" |
  Select-Object -ExpandProperty Name

foreach ($file in $runtimeFiles) {
  Copy-Item -LiteralPath (Join-Path $whisperRelease $file) -Destination $runtimeDestination -Force
}

$cublasRoot = Join-Path $cublasStaging "libcublas-windows-x86_64-11.8.1.74-archive"
Copy-Item -LiteralPath (Join-Path $cublasRoot "lib\cublas64_11.dll") -Destination $runtimeDestination -Force
Copy-Item -LiteralPath (Join-Path $cublasRoot "lib\cublasLt64_11.dll") -Destination $runtimeDestination -Force
Copy-Item -LiteralPath (Join-Path $cublasRoot "LICENSE") -Destination (Join-Path $runtimeDestination "NVIDIA-CUDA-LICENSE.txt") -Force

Write-Host "Provisioned pinned whisper.cpp v1.9.1 CUDA runtime."
Write-Host "Runtime: $runtimeDestination"
