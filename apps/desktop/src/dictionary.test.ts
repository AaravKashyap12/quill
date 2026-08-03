import { describe, expect, it } from "vitest";
import {
  capDismissedSuggestions,
  MAX_DISMISSED_SUGGESTIONS,
} from "./dictionary";

describe("dismissed dictionary suggestions", () => {
  it("keeps only the newest pairs at the persistence limit", () => {
    const suggestions = Array.from(
      { length: MAX_DISMISSED_SUGGESTIONS + 2 },
      (_, index) => ({
        spoken: `spoken-${index}`,
        replacement: `replacement-${index}`,
      }),
    );

    const capped = capDismissedSuggestions(suggestions);

    expect(capped).toHaveLength(MAX_DISMISSED_SUGGESTIONS);
    expect(capped[0]?.spoken).toBe("spoken-2");
    expect(capped.at(-1)?.spoken).toBe(
      `spoken-${MAX_DISMISSED_SUGGESTIONS + 1}`,
    );
  });
});
