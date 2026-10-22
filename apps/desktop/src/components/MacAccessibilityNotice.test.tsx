import type { ReactElement, ReactNode } from "react";
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
}));

function render() { harness.cursor = 0; return MacAccessibilityNotice(); }
function button(node: ReactElement | null): { onClick: () => Promise<void>; children: string } {
  const element = node as ReactElement<{ children?: ReactNode; onClick?: () => Promise<void> }>;
  if (element.type === "button" && element.props.onClick && typeof element.props.children === "string") {
    return { onClick: element.props.onClick, children: element.props.children };
  }
  for (const child of [element.props.children].flat()) {
    if (child && typeof child === "object") {
      try { return button(child as ReactElement); } catch { /* search remaining siblings */ }
    }
  }
  throw new Error("No permission button");
}

beforeEach(() => {
  harness.states = []; harness.cursor = 0; harness.effects = []; harness.focus = null;
  vi.clearAllMocks();
  vi.mocked(native.getMacosAccessibilityStatus).mockResolvedValue(false);
  vi.stubGlobal("window", {
    location: { search: "?platform=mac" },
    addEventListener: (_: string, callback: () => void) => { harness.focus = callback; },
    removeEventListener: () => { harness.focus = null; },
  });
});
afterEach(() => vi.unstubAllGlobals());

it("checks passively on mount and focus; only Enable requests OS consent", async () => {
  render();
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
