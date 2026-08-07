import type { SystemProfile } from "./types";

export type SetupLanguage = "english" | "multilingual";
export type SetupPriority = "fast" | "balanced" | "accurate";

export interface RecommendedModel {
  id: string;
  sizeLabel: string;
  bytes: number;
  reason: string;
}

const GIB = 1024 ** 3;

const MODEL_BYTES: Record<"base" | "small" | "medium", number> = {
  base: 147_951_465,
  small: 487_593_953,
  medium: 1_533_763_425,
};

function model(
  tier: keyof typeof MODEL_BYTES,
  language: SetupLanguage,
  reason: string,
): RecommendedModel {
  const englishSuffix = language === "english" ? ".en" : "";
  return {
    id: `${tier}${englishSuffix}`,
    sizeLabel: tier === "base" ? "142 MB" : tier === "small" ? "466 MB" : "1.5 GB",
    bytes: MODEL_BYTES[tier],
    reason,
  };
}

/**
 * Pick conservatively from memory that is actually available now, capped at
 * half of physical RAM so Quill never assumes it owns the whole computer.
 * The user preference selects the ceiling; hardware can only lower it.
 */
export function recommendSpeechModel(
  profile: SystemProfile,
  language: SetupLanguage,
  priority: SetupPriority,
): RecommendedModel {
  const totalBudget = profile.totalMemoryBytes * 0.5;
  const availableBudget = profile.availableMemoryBytes * 0.8;
  const usableMemory = Math.min(totalBudget, availableBudget) / GIB;
  const canRunSmall = usableMemory >= 4 && profile.logicalCpuCount >= 4;
  const canRunMedium = usableMemory >= 8 && profile.logicalCpuCount >= 8;

  if (priority === "fast") {
    return model("base", language, "Fast startup and the lightest load on your computer.");
  }
  if (priority === "accurate" && canRunMedium) {
    return model("medium", language, "Best accuracy this computer can run comfortably.");
  }
  if (canRunSmall) {
    return model(
      "small",
      language,
      priority === "accurate"
        ? "Strong accuracy without overloading this computer."
        : "The best balance of speed and accuracy for this computer.",
    );
  }
  return model("base", language, "A reliable fit for the memory currently available.");
}

export function modelDownloadBytes(id: string): number {
  const tier = id.replace(/\.en$/, "") as keyof typeof MODEL_BYTES;
  return MODEL_BYTES[tier] ?? 0;
}
