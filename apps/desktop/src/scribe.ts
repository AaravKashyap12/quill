import type { AppSettings, ProviderStatus } from "./types";

/** Scribe is ready only when the user's explicit model choice is available. */
export function isScribeReady(
  settings: Pick<AppSettings, "cleanupModel">,
  providers: readonly ProviderStatus[],
): boolean {
  const selected = settings.cleanupModel.trim();
  return (
    selected.length > 0 &&
    providers.some(
      (provider) => provider.available && provider.models.includes(selected),
    )
  );
}
