import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/components/ui/switch", () => ({
  Switch: ({ id, checked }: { id?: string; checked?: boolean }) => (
    <span id={id} data-checked={String(checked)} />
  ),
}));

import { ProviderAdvancedConfig } from "@/components/providers/forms/ProviderAdvancedConfig";

describe("ProviderAdvancedConfig", () => {
  it("accepts precise provider cost multipliers like 0.001", () => {
    render(
      <ProviderAdvancedConfig
        testConfig={{ enabled: false }}
        pricingConfig={{
          enabled: true,
          costMultiplier: "0.001",
          pricingModelSource: "inherit",
        }}
        onTestConfigChange={vi.fn()}
        onPricingConfigChange={vi.fn()}
      />,
    );

    const input = document.getElementById(
      "cost-multiplier",
    ) as HTMLInputElement;

    expect(input).toHaveAttribute("step", "any");
    expect(input.checkValidity()).toBe(true);
  });
});
