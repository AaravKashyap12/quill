import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import { GeneralView } from "./GeneralView";
import { defaultSettings, macShortcutPreset } from "../defaults";

afterEach(() => vi.unstubAllGlobals());

it.each(["apple-fn", "no-fn"] as const)("offers %s presets and restricts modifier shortcuts to Hold", (preset) => {
  vi.stubGlobal("window", { location: { search: "?platform=mac" } });
  const html = renderToStaticMarkup(<GeneralView
    settings={{ ...defaultSettings, dictationHotkey: macShortcutPreset("dictation", preset), scribeHotkey: macShortcutPreset("scribe", preset) }}
    update={vi.fn()} onHotkey={vi.fn()} onPreview={vi.fn()}
  />);
  expect(html).toContain('aria-label="Dictation shortcut preset"');
  expect(html).toContain('aria-label="Scribe shortcut preset"');
  expect(html.match(/disabled="">Tap/g)).toHaveLength(2);
  expect(html).toContain("Do Nothing");
  expect(html).toContain("No Fn");
});

it("leaves Windows shortcut controls and Tap available", () => {
  vi.stubGlobal("window", { location: { search: "?platform=win" } });
  const html = renderToStaticMarkup(<GeneralView settings={defaultSettings} update={vi.fn()} onHotkey={vi.fn()} onPreview={vi.fn()} />);
  expect(html).not.toContain("shortcut preset");
  expect(html).not.toContain('disabled="">Tap');
});
