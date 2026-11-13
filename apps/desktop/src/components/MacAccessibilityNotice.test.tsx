import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MacAccessibilityNotice } from "./MacAccessibilityNotice";
import * as native from "../tauri";

const harness = vi.hoisted(() => ({
  states: [] as unknown[], cursor: 0,
  effects: [] as Array<() => void | (() => void)>,
  focus: null as (() => void) | null,
}));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState: (initial: unknown) => {
    const index = harness.cursor++;
    if (index >= harness.states.length) harness.states.push(initial);
    return [harness.states[index], (next: unknown) => { harness.states[index] = next; }];
  },
  useEffect: (effect: () => void | (() => void)) => { harness.effects.push(effect); },
}));
vi.mock("../tauri", () => ({
  getMacosAccessibilityStatus: vi.fn(async () => false),
  requestMacosAccessibility: vi.fn(async () => false),
  openMacosAccessibilitySettings: vi.fn(async () => {}),
}));

function render() { harness.cursor = 0; return MacAccessibilityNotice(); }
function button(node: ReactElement | null, label?: string): { onClick: () => Promise<void>; children: string } {
  const element = node as ReactElement<{ children?: ReactNode; onClick?: () => Promise<void> }>;
  if (element.type === "button" && element.props.onClick && typeof element.props.children === "string" && (!label || element.props.children === label)) {
    return { onClick: element.props.onClick, children: element.props.children };
  }
  for (const child of [element.props.children].flat()) {
    if (child && typeof child === "object") {
      try { return button(child as ReactElement, label); } catch { /* search remaining siblings */ }
    }
  }
  throw new Error("No permission button");
}

beforeEach(() => {
  harness.states = []; harness.cursor = 0; harness.effects = []; harness.focus = null;
  vi.clearAllMocks();
  vi.mocked(native.getMacosAccessibilityStatus).mockResolvedValue(false);
  vi.mocked(native.openMacosAccessibilitySettings).mockResolvedValue(undefined);
  vi.stubGlobal("window", {
    location: { search: "?platform=mac" },
    addEventListener: (_: string, callback: () => void) => { harness.focus = callback; },
    removeEventListener: () => { harness.focus = null; },
  });
});
afterEach(() => vi.unstubAllGlobals());

it("checks passively on mount and focus; only Enable requests OS consent", async () => {
  expect(render()).toBeNull();
  const cleanups = harness.effects.splice(0).map((effect) => effect());
  await vi.waitFor(() => expect(native.getMacosAccessibilityStatus).toHaveBeenCalledOnce());
  expect(native.requestMacosAccessibility).not.toHaveBeenCalled();
  await button(render()).onClick();
  expect(native.requestMacosAccessibility).toHaveBeenCalledOnce();
  const pending = button(render());
  expect(pending.children).toBe("Check access");
  await pending.onClick();
  harness.focus!();
  expect(native.requestMacosAccessibility).toHaveBeenCalledOnce();
  expect(native.getMacosAccessibilityStatus).toHaveBeenCalledTimes(3);
  vi.mocked(native.getMacosAccessibilityStatus).mockResolvedValue(true);
  harness.focus!();
  await vi.waitFor(() => expect(render()).toBeNull());
  for (const cleanup of cleanups) cleanup?.();
  expect(harness.focus).toBeNull();
});

it("does not query or request Mac permissions on Windows", () => {
  window.location.search = "?platform=win";
  expect(render()).toBeNull();
  for (const effect of harness.effects.splice(0)) effect();
  expect(native.getMacosAccessibilityStatus).not.toHaveBeenCalled();
  expect(native.requestMacosAccessibility).not.toHaveBeenCalled();
});

it("explains how to recover when Check access still reports denied", async () => {
  render();
  for (const effect of harness.effects.splice(0)) effect();
  await vi.waitFor(() => expect(render()).not.toBeNull());
  await button(render()).onClick();
  await button(render()).onClick();
  const html = renderToStaticMarkup(render());
  expect(html).toContain("Access is still off for this copy of Quill.");
  expect(html).toContain("/Applications/Quill.app");
  expect(html).toContain("Quit Quill from the menu-bar icon");
  expect(html).toContain("Cmd+Q keeps Quill running in the background");
  expect(html).toContain("remove only Quill");
  expect(native.requestMacosAccessibility).toHaveBeenCalledOnce();
});

it("reopens privacy settings without repeating the consent prompt and reports open failures", async () => {
  render();
  for (const effect of harness.effects.splice(0)) effect();
  await vi.waitFor(() => expect(render()).not.toBeNull());
  await button(render()).onClick();
  const help = renderToStaticMarkup(render());
  expect(help).toContain("System Preferences");
  expect(help).toContain("Monterey");
  expect(help).toContain("Ventura and later");
  await button(render(), "Open privacy settings").onClick();
  expect(native.openMacosAccessibilitySettings).toHaveBeenCalledOnce();
  expect(native.requestMacosAccessibility).toHaveBeenCalledOnce();
  vi.mocked(native.openMacosAccessibilitySettings).mockRejectedValueOnce(new Error("Settings could not open"));
  await button(render(), "Open privacy settings").onClick();
  expect(renderToStaticMarkup(render())).toContain("Settings could not open");
});
