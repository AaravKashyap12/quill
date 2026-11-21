import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, expect, it, vi } from "vitest";
import { LocalScribeModel } from "./LocalScribeModel";
import * as native from "../tauri";

const hooks = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0, effects: [] as Array<() => void | (() => void)> }));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState: (initial: unknown) => {
    const index = hooks.cursor++;
    if (index >= hooks.values.length) hooks.values.push(initial);
    return [hooks.values[index], (next: unknown) => { hooks.values[index] = typeof next === "function" ? next(hooks.values[index]) : next; }];
  },
  useRef: (current: unknown) => ({ current }),
  useEffect: (effect: () => void | (() => void)) => hooks.effects.push(effect),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../tauri", () => ({ getLocalScribeStatus: vi.fn(), downloadLocalScribe: vi.fn(), pauseLocalScribe: vi.fn(), deleteLocalScribe: vi.fn() }));
const change = vi.fn();
function render() { hooks.cursor = 0; return LocalScribeModel({ onChange: change }); }
function action(node: ReactNode, label: string): () => void {
  if (!node || typeof node !== "object" || !("props" in node)) throw Error("No button");
  const element = node as ReactElement<{ children?: ReactNode; onClick?: () => void }>;
  if (element.type === "button" && renderToStaticMarkup(element).includes(label)) return element.props.onClick!;
  for (const child of [element.props.children].flat()) { try { return action(child, label); } catch { /* next sibling */ } }
  throw Error("No button");
}
beforeEach(() => {
  hooks.values = []; hooks.cursor = 0; hooks.effects = [];
  vi.resetAllMocks();
  vi.mocked(native.getLocalScribeStatus).mockResolvedValue({ installed: false, downloadBytes: 100, partialBytes: 50, downloading: false, engineAvailable: true, engineError: null });
  vi.mocked(native.downloadLocalScribe).mockResolvedValue();
  vi.mocked(native.pauseLocalScribe).mockResolvedValue();
  vi.mocked(native.deleteLocalScribe).mockResolvedValue();
});

it("checks without downloading and resumes only on the user's click", async () => {
  render();
  const cleanups = hooks.effects.splice(0).map((effect) => effect());
  await vi.waitFor(() => expect(renderToStaticMarkup(render())).toContain("Resume download · 50% saved"));
  expect(native.downloadLocalScribe).not.toHaveBeenCalled();
  expect(native.deleteLocalScribe).not.toHaveBeenCalled();
  action(render(), "Resume download")();
  await vi.waitFor(() => expect(change).toHaveBeenCalled());
  expect(native.downloadLocalScribe).toHaveBeenCalledOnce();
  for (const cleanup of cleanups) cleanup?.();
});

it("offers pause when reopening settings during a download", async () => {
  vi.mocked(native.getLocalScribeStatus).mockResolvedValue({ installed: false, downloadBytes: 100, partialBytes: 25, downloading: true, engineAvailable: true, engineError: null });
  render();
  const cleanups = hooks.effects.splice(0).map((effect) => effect());
  await vi.waitFor(() => expect(renderToStaticMarkup(render())).toContain("Pause download"));
  action(render(), "Pause download")();
  await vi.waitFor(() => expect(native.pauseLocalScribe).toHaveBeenCalledOnce());
  expect(native.downloadLocalScribe).not.toHaveBeenCalled();
  for (const cleanup of cleanups) cleanup?.();
});
