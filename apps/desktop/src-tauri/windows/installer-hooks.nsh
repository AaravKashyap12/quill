; Bridge older Quill releases whose updater launched NSIS while the bundled
; whisper-server process was still running. Restrict termination to a server
; whose executable lives under this Quill installation; another application's
; whisper-server.exe must never be touched.
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping Quill's local speech engine..."
  ; NSIS is a 32-bit process even for an x64 bundle. Without disabling WOW64
  ; file-system redirection, `powershell.exe` resolves to the 32-bit host,
  ; which cannot reliably read the executable path of the 64-bit sidecar.
  ; That made the old best-effort hook silently miss the process.
  ${If} ${RunningX64}
    ${DisableX64FSRedirection}
  ${EndIf}
  nsExec::ExecToStack `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$ErrorActionPreference = 'Stop'; $$root = [IO.Path]::GetFullPath('$INSTDIR\'); $$targets = @(Get-Process -Name whisper-server -ErrorAction SilentlyContinue | Where-Object { try { $$_.Path -and [IO.Path]::GetFullPath($$_.Path).StartsWith($$root, [StringComparison]::OrdinalIgnoreCase) } catch { $$false } }); if ($$targets.Count -gt 0) { $$targets | Stop-Process -Force; $$targets | Wait-Process -Timeout 10 }; exit 0"`
  Pop $0
  Pop $1
  ${If} ${RunningX64}
    ${EnableX64FSRedirection}
  ${EndIf}
  ${If} $0 != 0
    DetailPrint "Could not stop Quill's local speech engine."
    MessageBox MB_ICONSTOP|MB_OK "Quill could not stop its local speech engine. Close Quill from the tray, then run the update again."
    Abort
  ${EndIf}
!macroend
