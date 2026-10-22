import type { ChangeEvent } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { hotkeyFromEvent, releasedModifierHotkey, standaloneModifierKey, MacShortcutPreset } from "./ModeShortcut";
import { defaultSettings, formatHotkey, isModifierOnlyMacHotkey, macShortcutPreset } from "../defaults";

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
    expect(formatHotkey({ ...defaultSettings, dictationHotkey: hotkey! }, "dictation")).toBe("Right Option ⌥");
    expect(formatHotkey({ ...defaultSettings, scribeHotkey: { modifiers: [], key: "MetaRight", behavior: "hold" } }, "scribe")).toBe("Right Command ⌘");
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
    ["mac", "Fn", "Fn", []],
    ["win", "Space", "Space", ["Ctrl"]],
  ])("uses platform-specific defaults for %s", async (name, dictation, scribe, modifiers) => {
    platform(name as string);
    vi.resetModules();
    const { defaultSettings: defaults, isModifierOnlyMacHotkey } = await import("../defaults");
    expect(defaults.dictationHotkey).toEqual({ modifiers, key: dictation, behavior: "hold" });
    expect(defaults.scribeHotkey.key).toBe(scribe);
    expect(isModifierOnlyMacHotkey(defaults.dictationHotkey)).toBe(name === "mac");
    expect(defaults.scribeHotkey.modifiers).toEqual(name === "mac" ? ["Ctrl"] : ["Ctrl", "Shift"]);
  });
});

describe("Mac preset selection", () => {
  it.each(["dictation", "scribe"] as const)("selects %s presets without Fn keyboard events", (mode) => {
    platform("mac");
    const onChange = vi.fn();
    const select = MacShortcutPreset({ mode, settings: defaultSettings, onChange });
    select.props.onChange({ target: { value: "apple-fn" } } as ChangeEvent<HTMLSelectElement>);
    expect(onChange).toHaveBeenLastCalledWith({ key: "Fn", modifiers: mode === "dictation" ? [] : ["Ctrl"], behavior: "hold" });
    select.props.onChange({ target: { value: "no-fn" } } as ChangeEvent<HTMLSelectElement>);
    expect(onChange).toHaveBeenLastCalledWith({ key: "Alt", modifiers: mode === "dictation" ? ["Ctrl"] : ["Meta", "Ctrl"], behavior: "hold" });
    const settings = { ...defaultSettings, [mode === "dictation" ? "dictationHotkey" : "scribeHotkey"]: onChange.mock.lastCall![0] };
    expect(MacShortcutPreset({ mode, settings, onChange }).props.value).toBe("no-fn");
    expect(isModifierOnlyMacHotkey(onChange.mock.lastCall![0])).toBe(true);
  });

  it("displays a custom binding without replacing it and preserves letter recording", () => {
    platform("mac");
    const custom = { key: "D", modifiers: ["Meta"], behavior: "tap-to-lock" as const };
    const onChange = vi.fn();
    const select = MacShortcutPreset({ mode: "dictation", settings: { ...defaultSettings, dictationHotkey: custom }, onChange });
    expect(select.props.value).toBe("custom");
    expect(onChange).not.toHaveBeenCalled();
    expect(isModifierOnlyMacHotkey(custom)).toBe(false);
    expect(hotkeyFromEvent({ ...event, key: "D", altKey: false, metaKey: true }, "tap-to-lock")).toEqual({ ok: true, hotkey: custom });
  });

  it("labels Fn and Control clearly, and does not misrecord Fn as a character", () => {
    platform("mac");
    expect(formatHotkey({ ...defaultSettings, scribeHotkey: macShortcutPreset("scribe", "apple-fn") }, "scribe")).toBe("Ctrl ⌃ + Fn");
    expect(hotkeyFromEvent({ ...event, key: "Fn", altKey: false }, "hold")).toBeNull();
    expect(isModifierOnlyMacHotkey(macShortcutPreset("scribe", "apple-fn"))).toBe(true);
    platform("win");
    expect(isModifierOnlyMacHotkey(macShortcutPreset("scribe", "no-fn"))).toBe(false);
  });
});
