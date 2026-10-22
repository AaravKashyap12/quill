import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { defaultSettings } from "./defaults";
import { getProviderKeyStatus, testProviderConnection, mergeProviderKeyStatuses } from "./tauri";
import type { ProviderKeyStatus } from "./types";
import { VoiceView } from "./views/VoiceView";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: vi.fn() }));
beforeEach(() => vi.resetAllMocks());

describe("credential access recovery", () => {
  it("keeps successful connection tests through passive vault refreshes", () => {
    const connected: ProviderKeyStatus = { provider: "gemini", configured: true, status: "connected", message: null };
    const configured: ProviderKeyStatus = { ...connected, status: "configured" };
    expect(mergeProviderKeyStatuses([], [configured], connected)).toEqual([connected]);
    expect(mergeProviderKeyStatuses([connected], [configured])).toEqual([connected]);
    const rejected: ProviderKeyStatus = { ...connected, status: "error", message: "Key rejected" };
    expect(mergeProviderKeyStatuses([connected], [configured], rejected)).toEqual([rejected]);
  });

  it("invalidates connection readiness on missing or inaccessible keys", () => {
    const connected: ProviderKeyStatus = { provider: "gemini", configured: true, status: "connected", message: null };
    for (const status of ["missing", "error"] as const) {
      const unavailable: ProviderKeyStatus = { ...connected, configured: false, status };
      expect(mergeProviderKeyStatuses([connected], [unavailable])).toEqual([unavailable]);
    }
  });
  it("returns an actionable status on denial without retrying the OS store", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Keychain access denied");
    expect(await getProviderKeyStatus("groq")).toEqual({
      provider: "groq", configured: false, status: "error", message: "Keychain access denied",
    });
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("retries credential access only when deliberately requested", async () => {
    await testProviderConnection("groq");
    expect(invoke).toHaveBeenLastCalledWith("test_provider_connection", { provider: "groq", retryAccess: false });
    await testProviderConnection("groq", true);
    expect(invoke).toHaveBeenLastCalledWith("test_provider_connection", { provider: "groq", retryAccess: true });
  });

  it("keeps status passive unless initial access is explicitly requested", async () => {
    await getProviderKeyStatus("gemini");
    expect(invoke).toHaveBeenLastCalledWith("get_provider_key_status", { provider: "gemini", requestAccess: false });
    await getProviderKeyStatus("groq", true);
    expect(invoke).toHaveBeenLastCalledWith("get_provider_key_status", { provider: "groq", requestAccess: true });
  });

  it("offers an enabled saved-key check without claiming an unchecked key is missing", () => {
    const html = renderToStaticMarkup(<VoiceView
      settings={{ ...defaultSettings, cleanupProvider: "gemini" }}
      update={() => {}} providers={[]}
      cloudStatuses={[{ provider: "gemini", configured: false, status: "unchecked", message: null }]}
      onDetectProviders={async () => {}} onCloudStatusChange={async () => {}}
    />);
    expect(html).toMatch(/<button[^>]*>Check saved key<\/button>/);
    expect(html).not.toMatch(/<button[^>]*disabled[^>]*>Check saved key<\/button>/);
    expect(html).toContain("Gemini access not checked");
    expect(html).not.toContain("Gemini key needed");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("offers a retry without requiring a replacement key after denied access", () => {
    const html = renderToStaticMarkup(<VoiceView
      settings={{ ...defaultSettings, transcriptionProvider: "groq" }}
      update={() => {}} providers={[]}
      cloudStatuses={[{ provider: "groq", configured: false, status: "error", message: "Keychain <denied>" }]}
      onDetectProviders={async () => {}} onCloudStatusChange={async () => {}}
    />);
    expect(html).toMatch(/<button[^>]*>Retry access<\/button>/);
    expect(html).not.toMatch(/<button[^>]*disabled[^>]*>Retry access<\/button>/);
    expect(html).toContain("Keychain &lt;denied&gt;");
    expect(invoke).not.toHaveBeenCalled();
  });
});
