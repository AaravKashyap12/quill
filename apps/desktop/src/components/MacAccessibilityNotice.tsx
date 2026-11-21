import { useEffect, useState } from "react";
import { isMac } from "../platform";
import { getMacosAccessibilityStatus, openMacosAccessibilitySettings, requestMacosAccessibility } from "../tauri";

export function MacAccessibilityNotice() {
  const [granted, setGranted] = useState<boolean | null>(null);
  const [requested, setRequested] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isMac()) return;
    let disposed = false;
    const check = () => {
      void getMacosAccessibilityStatus().then((allowed) => {
        if (!disposed) { setGranted(allowed); setError(null); }
      }).catch((reason: unknown) => {
        if (!disposed) setError(String(reason));
      });
    };
    check();
    window.addEventListener("focus", check);
    return () => { disposed = true; window.removeEventListener("focus", check); };
  }, []);

  async function enable() {
    setBusy(true);
    setError(null);
    try {
      // A false result means the asynchronous OS decision is still pending.
      setGranted(await requestMacosAccessibility());
      setRequested(true);
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function recheck() {
    setBusy(true);
    try {
      const allowed = await getMacosAccessibilityStatus();
      setGranted(allowed);
      setError(allowed ? null : "Access is still off for this copy of Quill. Enable /Applications/Quill.app in macOS privacy settings, then choose Quit Quill from the menu-bar icon and reopen it. Cmd+Q keeps Quill running in the background. If access is still denied after an update, remove only Quill from that list with the minus button, add the current app with the plus button, and enable it again.");
    }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function openSettings() {
    setBusy(true);
    setError(null);
    try { await openMacosAccessibilitySettings(); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  if (!isMac() || granted || (granted === null && !error)) return null;
  return (
    <section className="settings-group" aria-label="Text insertion permission">
      <div className="setting-row">
        <div className="setting-copy">
          <strong className="setting-label">Allow text insertion</strong>
          <p className="setting-description">Quill uses Accessibility to insert into your original text field.</p>
          {requested && <p className="setting-description" role="status">Enable Quill under Accessibility: System Preferences → Security &amp; Privacy → Privacy on Monterey, or System Settings → Privacy &amp; Security on Ventura and later (Device Control and Data Access if your version uses that label). Then return here.</p>}
          {error && <p className="setting-description" role="alert">{error}</p>}
        </div>
        <div className="inline-control permission-actions">
          <button type="button" className="row-download-button" disabled={busy} onClick={requested ? recheck : enable}>
            {requested ? "Check access" : "Enable text insertion"}
          </button>
          {requested && <button type="button" className="row-download-button" disabled={busy} onClick={openSettings}>Open privacy settings</button>}
        </div>
      </div>
    </section>
  );
}
