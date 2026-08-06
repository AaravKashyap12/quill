export type DesktopPlatform = "win" | "mac" | "linux";

export function detectPlatform(): DesktopPlatform {
  if (typeof window !== "undefined") {
    const previewPlatform = new URLSearchParams(window.location.search).get("platform");
    if (previewPlatform === "mac" || previewPlatform === "linux" || previewPlatform === "win") {
      return previewPlatform;
    }
  }
  const platform = (
    typeof navigator !== "undefined" ? navigator.platform ?? "" : ""
  ).toLowerCase();
  if (platform.startsWith("mac")) return "mac";
  if (platform.startsWith("linux")) return "linux";
  return "win";
}

export function isMac(): boolean {
  return detectPlatform() === "mac";
}
