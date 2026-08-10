import type { AppSettings, ProviderKeyStatus, ProviderStatus } from "./types";

/** Scribe is ready only when the user's explicit model choice is available. */
export function isScribeReady(
  settings: Pick<AppSettings, "cleanupModel"> & Partial<Pick<AppSettings, "cleanupProvider">>,
  providers: readonly ProviderStatus[],
  cloudStatuses: readonly ProviderKeyStatus[] = [],
): boolean {
  if (settings.cleanupProvider === "disabled") return false;
  if (settings.cleanupProvider === "gemini") {
    return cloudStatuses.some(
      (status) => status.provider === "gemini" && status.configured && status.status === "connected",
    );
  }
  const selected = settings.cleanupModel.trim();
  return (
    selected.length > 0 &&
    providers.some(
      (provider) => provider.available && provider.models.includes(selected),
    )
  );
}
