import type { DictionarySuggestion } from "./types";

export const MAX_DISMISSED_SUGGESTIONS = 200;

export function capDismissedSuggestions(
  suggestions: DictionarySuggestion[],
): DictionarySuggestion[] {
  return suggestions.slice(-MAX_DISMISSED_SUGGESTIONS);
}

export function createDictionaryEntryId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  if (typeof crypto !== "undefined" && typeof crypto.getRandomValues === "function") {
    const random = crypto.getRandomValues(new Uint32Array(4));
    return `dictionary-${Array.from(random, (value) => value.toString(36)).join("-")}`;
  }
  return `dictionary-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`;
}
