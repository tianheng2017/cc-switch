import { describe, expect, it } from "vitest";
import {
  formatUsageDataSummary,
  formatUsageResetTime,
} from "@/utils/usageDisplay";

const labels = {
  invalid: "Invalid",
  remaining: "Remaining:",
  fiveHourRemaining: "5h remaining:",
  weeklyRemainingQuota: "Weekly remaining:",
  cycleEndsAt: "7d reset:",
  windowEndsAt: "5h reset:",
  used: "Used:",
};

describe("formatUsageDataSummary", () => {
  it("formats used percentage when remaining is omitted", () => {
    expect(
      formatUsageDataSummary(
        {
          planName: "Coco OpenRouter",
          used: 55,
          total: 100,
          unit: "%",
        },
        labels,
      ),
    ).toBe("[Coco OpenRouter] Used: 55%");
  });

  it("formats remaining when present", () => {
    expect(
      formatUsageDataSummary(
        {
          planName: "Balance",
          remaining: 12.5,
          unit: "USD",
        },
        labels,
      ),
    ).toBe("[Balance] Remaining: 12.50 USD");
  });

  it("formats invalid results without requiring quota fields", () => {
    expect(
      formatUsageDataSummary(
        {
          isValid: false,
          invalidMessage: "Unauthorized",
        },
        labels,
      ),
    ).toBe("Unauthorized");
  });

  it("formats windowed quota fields with 5h and weekly labels", () => {
    const expectedUtcReset = formatUsageResetTime("2026-05-30T12:00:00Z");
    const expectedLocalReset = formatUsageResetTime(
      "2026-06-01T23:33:05.593484",
    );

    expect(
      formatUsageDataSummary(
        {
          windowRemainingQuota: 10,
          weeklyRemainingQuota: 42.25,
          windowEndsAt: "2026-05-30T12:00:00Z",
          cycleEndsAt: "2026-06-01T23:33:05.593484",
        },
        labels,
      ),
    ).toBe(
      `5h remaining: 10 / Weekly remaining: 42.25 / 5h reset: ${expectedUtcReset} / 7d reset: ${expectedLocalReset}`,
    );
  });

  it("formats timezone-aware reset strings in local time", () => {
    expect(formatUsageResetTime("2026-05-30T12:00:00+08:00")).toBe(
      formatUsageResetTime(new Date("2026-05-30T04:00:00Z").getTime()),
    );
  });
});
