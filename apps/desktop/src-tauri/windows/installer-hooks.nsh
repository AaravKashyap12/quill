; Bridge older Quill releases whose updater launched NSIS while the bundled
; whisper-server process was still running. Restrict termination to a server
; whose executable lives under this Quill installation; another application's
; whisper-server.exe must never be touched.
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping Quill's local speech engine..."
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$root = [IO.Path]::GetFullPath('$INSTDIR\'); Get-Process -Name whisper-server -ErrorAction SilentlyContinue | Where-Object { try { $$_.Path -and [IO.Path]::GetFullPath($$_.Path).StartsWith($$root, [StringComparison]::OrdinalIgnoreCase) } catch { $$false } } | Stop-Process -Force -ErrorAction SilentlyContinue"`
  Sleep 500
!macroend
