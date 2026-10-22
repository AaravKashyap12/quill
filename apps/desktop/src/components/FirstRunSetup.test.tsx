import { Children, isValidElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { FirstRunSetup } from "./FirstRunSetup";

function primaryButton(node: ReactNode): { onClick: () => void; children: ReactNode } | undefined {
  for (const child of Children.toArray(node)) {
    if (!isValidElement<{ className?: string; onClick: () => void; children: ReactNode }>(child)) continue;
    if (child.type === "button" && child.props.className?.includes("primary-button")) return child.props;
    const found = primaryButton(child.props.children);
    if (found) return found;
  }
}

describe("General speech setup recovery", () => {
  it.each(["groq", "gemini", "local"] as const)("routes %s failure to the correct recovery action", (speechProvider) => {
    const download = vi.fn();
    const openVoice = vi.fn();
    const tree = FirstRunSetup({
      speechProvider,
      speech: { phase: "error", modelId: speechProvider === "local" ? "base.en" : "Cloud speech", bytesDownloaded: 0, bytesTotal: 0, error: "Access denied" },
      showScribeSetup: false, onOpenVoice: openVoice, onRetrySpeech: download, onDismissScribe: vi.fn(),
    });
    const primary = primaryButton(tree);
    expect(primary).toBeDefined();
    primary!.onClick();
    if (speechProvider === "local") {
      expect(download).toHaveBeenCalledOnce();
      expect(openVoice).not.toHaveBeenCalled();
    } else {
      expect(download).not.toHaveBeenCalled();
      expect(openVoice).toHaveBeenCalledWith("speech");
      expect(Children.toArray(primary!.children).join("")).toContain("Connect speech provider");
    }
  });
});
