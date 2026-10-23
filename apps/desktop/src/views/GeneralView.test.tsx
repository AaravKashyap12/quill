import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import { GeneralView } from "./GeneralView";
import { defaultSettings, macShortcutPreset } from "../defaults";
import { isValidElement, type ReactElement, type ReactNode } from "react";

function tapButtons(node: ReactNode): ReactElement<{ children?: ReactNode; onClick?: () => void; disabled?: boolean }>[] {
  if (Array.isArray(node)) return node.flatMap(tapButtons);
  if (!isValidElement<{ children?: ReactNode; onClick?: () => void; disabled?: boolean }>(node)) return [];
  return node.type === "button" && node.props.children === "Tap" ? [node] : tapButtons(node.props.children);
}

it("switches both Mac modes to Tap while preserving their keys", () => {
  vi.stubGlobal("window", { location: { search: "?platform=mac" } });
  const settings = { ...defaultSettings, dictationHotkey: macShortcutPreset("dictation", "apple-fn"), scribeHotkey: macShortcutPreset("scribe", "apple-fn") };
  const onHotkey = vi.fn();
  const buttons = tapButtons(GeneralView({ settings, update: vi.fn(), onHotkey, onPreview: vi.fn() }));
  expect(buttons).toHaveLength(2);
  for (const button of buttons) {
    expect(button.props.disabled).not.toBe(true);
    button.props.onClick!();
  }
  expect(onHotkey.mock.calls).toEqual([
    ["dictation", { ...settings.dictationHotkey, behavior: "tap-to-lock" }],
    ["scribe", { ...settings.scribeHotkey, behavior: "tap-to-lock" }],
  ]);
});

afterEach(() => vi.unstubAllGlobals());

it.each(["apple-fn", "no-fn"] as const)("offers %s presets with both Hold and Tap available", (preset) => {
  vi.stubGlobal("window", { location: { search: "?platform=mac" } });
  const html = renderToStaticMarkup(<GeneralView
    settings={{ ...defaultSettings, dictationHotkey: macShortcutPreset("dictation", preset), scribeHotkey: macShortcutPreset("scribe", preset) }}
    update={vi.fn()} onHotkey={vi.fn()} onPreview={vi.fn()}
  />);
  expect(html).toContain('aria-label="Dictation shortcut preset"');
  expect(html).toContain('aria-label="Scribe shortcut preset"');
  expect(html).not.toContain('disabled="">Tap');
  expect(html).toContain("Do Nothing");
  expect(html).toContain("No Fn");
});

it("leaves Windows shortcut controls and Tap available", () => {
  vi.stubGlobal("window", { location: { search: "?platform=win" } });
  const html = renderToStaticMarkup(<GeneralView settings={defaultSettings} update={vi.fn()} onHotkey={vi.fn()} onPreview={vi.fn()} />);
  expect(html).not.toContain("shortcut preset");
  expect(html).not.toContain('disabled="">Tap');
});
