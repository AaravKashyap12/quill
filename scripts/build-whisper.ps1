param(
  [string]$CacheDirectory = (Join-Path $env:TEMP "quill-whisper"),
  [switch]$ForceDownload
)

$ErrorActionPreference = "Stop"
$workspace = Resolve-Path (Join-Path $PSScriptRoot "..")
$resourceRoot = Join-Path $workspace "apps\desktop\src-tauri\resources\whisper"
$cpuDestination = Join-Path $resourceRoot "windows-x64-cpu"
$artifactDestination = Join-Path $workspace "target\release-assets"
$cudaAssetName = "quill-cuda-runtime-windows-x64.zip"
$cudaArchive = Join-Path $artifactDestination $cudaAssetName
$cudaChecksum = "$cudaArchive.sha256"
$whisperLicense = Join-Path $resourceRoot "WHISPER-LICENSE.txt"
$provenance = Get-Content -Raw (Join-Path $resourceRoot "manifest.json") | ConvertFrom-Json

$artifacts = @{
  Whisper = @{
    Url = "https://github.com/ggml-org/whisper.cpp/releases/download/$($provenance.whisperCpp.version)/$($provenance.whisperCpp.asset)"
    File = $provenance.whisperCpp.asset
    Sha256 = $provenance.whisperCpp.sha256
  }
  Cublas = @{
    Url = "https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/$($provenance.cudaRuntime.asset)"
    File = $provenance.cudaRuntime.asset
    Sha256 = $provenance.cudaRuntime.sha256
  }
}

function Get-VerifiedArtifact {
  param([hashtable]$Artifact)

  New-Item -ItemType Directory -Force -Path $CacheDirectory | Out-Null
  $path = Join-Path $CacheDirectory $Artifact.File
  if ($ForceDownload -or -not (Test-Path -LiteralPath $path)) {
    try {
      Invoke-WebRequest -Uri $Artifact.Url -OutFile $path
    } catch {
      Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
      throw "Failed to download pinned artifact $($Artifact.File)"
    }
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
$cudaStaging = Join-Path $staging "cuda-pack"

if (-not (Test-Path -LiteralPath $whisperLicense -PathType Leaf)) {
  throw "whisper.cpp licence notice is missing: $whisperLicense"
}

if (Test-Path -LiteralPath $cpuDestination) {
  Remove-Item -LiteralPath $cpuDestination -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $whisperStaging, $cublasStaging, $cudaStaging, $cpuDestination, $artifactDestination | Out-Null

try {
  Expand-Archive -LiteralPath $whisperArchive -DestinationPath $whisperStaging
  Expand-Archive -LiteralPath $cublasArchive -DestinationPath $cublasStaging

  $whisperRelease = Join-Path $whisperStaging "Release"
  $cpuFiles = @(
    "whisper-server.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll"
  )
  $cpuFiles += Get-ChildItem -LiteralPath $whisperRelease -Filter "ggml-cpu-*.dll" |
    Select-Object -ExpandProperty Name

  foreach ($file in $cpuFiles) {
    Copy-Item -LiteralPath (Join-Path $whisperRelease $file) -Destination $cpuDestination -Force
  }
  Copy-Item -LiteralPath $whisperLicense -Destination $cpuDestination -Force

  $cudaFiles = @(
    "ggml-cuda.dll",
    "cudart32_110.dll",
    "cudart64_110.dll",
    "cuinj64_118.dll",
    "nvrtc-builtins64_118.dll",
    "nvrtc64_112_0.dll"
  )
  foreach ($file in $cudaFiles) {
    Copy-Item -LiteralPath (Join-Path $whisperRelease $file) -Destination $cudaStaging -Force
  }

  $cublasRoot = Join-Path $cublasStaging ([System.IO.Path]::GetFileNameWithoutExtension($provenance.cudaRuntime.asset))
  Copy-Item -LiteralPath (Join-Path $cublasRoot "bin\cublas64_11.dll") -Destination $cudaStaging -Force
  Copy-Item -LiteralPath (Join-Path $cublasRoot "bin\cublasLt64_11.dll") -Destination $cudaStaging -Force
  Copy-Item -LiteralPath (Join-Path $cublasRoot "LICENSE") -Destination (Join-Path $cudaStaging "NVIDIA-CUDA-LICENSE.txt") -Force
  Copy-Item -LiteralPath $whisperLicense -Destination $cudaStaging -Force

  $packManifest = [ordered]@{
    schemaVersion = 1
    whisperVersion = $provenance.whisperCpp.version
    whisperRevision = $provenance.whisperCpp.revision
    platform = "windows-x64"
    backend = "cuda"
  } | ConvertTo-Json
  $utf8WithoutBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText(
    (Join-Path $cudaStaging "runtime-manifest.json"),
    $packManifest,
    $utf8WithoutBom
  )

  foreach ($required in @("whisper-server.exe", "whisper.dll", "ggml.dll", "ggml-base.dll")) {
    if (-not (Test-Path -LiteralPath (Join-Path $cpuDestination $required) -PathType Leaf)) {
      throw "CPU runtime is incomplete: $required is missing"
    }
  }
  foreach ($required in @("ggml-cuda.dll", "cublas64_11.dll", "cublasLt64_11.dll", "runtime-manifest.json")) {
    if (-not (Test-Path -LiteralPath (Join-Path $cudaStaging $required) -PathType Leaf)) {
      throw "CUDA runtime is incomplete: $required is missing"
    }
  }

  Remove-Item -LiteralPath $cudaArchive, $cudaChecksum -Force -ErrorAction SilentlyContinue
  Compress-Archive -Path (Join-Path $cudaStaging "*") -DestinationPath $cudaArchive -CompressionLevel Optimal
  $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cudaArchive).Hash.ToLowerInvariant()
  [System.IO.File]::WriteAllText(
    $cudaChecksum,
    "$archiveHash  $cudaAssetName`n",
    [System.Text.Encoding]::ASCII
  )

  $cpuBytes = (Get-ChildItem -LiteralPath $cpuDestination -File | Measure-Object Length -Sum).Sum
  $cudaBytes = (Get-ChildItem -LiteralPath $cudaStaging -File | Measure-Object Length -Sum).Sum
  Write-Host "Provisioned pinned whisper.cpp $($provenance.whisperCpp.version) CPU runtime."
  Write-Host "CPU runtime: $cpuDestination ($cpuBytes bytes)"
  Write-Host "CUDA archive: $cudaArchive ($((Get-Item -LiteralPath $cudaArchive).Length) bytes)"
  Write-Host "CUDA unpacked: $cudaBytes bytes"
  Write-Host "CUDA SHA-256: $archiveHash"
} finally {
  if (Test-Path -LiteralPath $staging) {
    Remove-Item -LiteralPath $staging -Recurse -Force
  }
}
