import { afterEach, describe, expect, it, vi } from "vitest";
import { hotkeyFromEvent, releasedModifierHotkey, standaloneModifierKey } from "./ModeShortcut";
import { defaultSettings, formatHotkey } from "../defaults";

const event = { code: "AltRight", key: "Alt", altKey: true, metaKey: false, ctrlKey: false, shiftKey: false };
afterEach(() => vi.unstubAllGlobals());
function platform(name: string) { vi.stubGlobal("window", { location: { search: `?platform=${name}` } }); }

describe("Mac right modifier shortcut recording", () => {
  it("waits for release and records a hold, with a readable label", () => {
    platform("mac");
    expect(hotkeyFromEvent(event, "hold")).toBeNull();
    const candidate = standaloneModifierKey(event);
    const hotkey = releasedModifierHotkey(candidate, { ...event, altKey: false });
    expect(hotkey).toEqual({ modifiers: [], key: "AltRight", behavior: "hold" });
    expect(formatHotkey({ ...defaultSettings, dictationHotkey: hotkey! }, "dictation")).toBe("Right Option");
    expect(formatHotkey({ ...defaultSettings, scribeHotkey: { modifiers: [], key: "MetaRight", behavior: "hold" } }, "scribe")).toBe("Right Command");
  });

  it("preserves right-modifier chords and refuses other modifiers as standalone", () => {
    platform("mac");
    expect(hotkeyFromEvent({ ...event, key: "D" }, "tap-to-lock")).toEqual({
      ok: true, hotkey: { modifiers: ["Alt"], key: "D", behavior: "tap-to-lock" },
    });
    expect(standaloneModifierKey({ ...event, code: "KeyD" })).toBeNull();
    expect(standaloneModifierKey({ ...event, code: "AltLeft" })).toBeNull();
    expect(standaloneModifierKey({ ...event, ctrlKey: true })).toBeNull();
    expect(releasedModifierHotkey(null, event)).toBeNull();
    expect(releasedModifierHotkey("MetaRight", event)).toBeNull();
  });

  it("keeps Windows modifier-only presses unsupported", () => {
    platform("win");
    expect(standaloneModifierKey(event)).toBeNull();
    expect(releasedModifierHotkey("AltRight", event)).toBeNull();
    expect(hotkeyFromEvent({ ...event, key: "D", metaKey: true }, "hold")?.ok).toBe(false);
  });
});

describe("platform defaults", () => {
  it.each([
    ["mac", "AltRight", "MetaRight", []],
    ["win", "Space", "Space", ["Ctrl"]],
  ])("uses platform-specific defaults for %s", async (name, dictation, scribe, modifiers) => {
    platform(name as string);
    vi.resetModules();
    const { defaultSettings: defaults, isStandaloneMacHotkey } = await import("../defaults");
    expect(defaults.dictationHotkey).toEqual({ modifiers, key: dictation, behavior: "hold" });
    expect(defaults.scribeHotkey.key).toBe(scribe);
    expect(isStandaloneMacHotkey(defaults.dictationHotkey)).toBe(name === "mac");
    if (name === "win") expect(defaults.scribeHotkey.modifiers).toEqual(["Ctrl", "Shift"]);
  });
});
