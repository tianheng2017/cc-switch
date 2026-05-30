import type { UsageData } from "@/types";

interface UsageSummaryLabels {
  invalid: string;
  remaining: string;
  fiveHourRemaining?: string;
  weeklyRemainingQuota?: string;
  cycleEndsAt?: string;
  windowEndsAt?: string;
  used: string;
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? value.toString() : value.toFixed(2);
}

function formatValue(value: number, unit?: string): string {
  if (!unit) {
    return formatNumber(value);
  }

  return unit === "%"
    ? `${formatNumber(value)}%`
    : `${formatNumber(value)} ${unit}`;
}

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

export function getWeeklyRemainingQuota(data: UsageData): number | undefined {
  if (isNumber(data.weeklyRemainingQuota)) {
    return data.weeklyRemainingQuota;
  }
  return undefined;
}

export function getWindowRemainingQuota(data: UsageData): number | undefined {
  if (isNumber(data.windowRemainingQuota)) {
    return data.windowRemainingQuota;
  }
  return undefined;
}

export function hasWindowedQuota(data: UsageData): boolean {
  return (
    getWindowRemainingQuota(data) !== undefined ||
    getWeeklyRemainingQuota(data) !== undefined ||
    data.cycleEndsAt !== undefined ||
    data.windowEndsAt !== undefined
  );
}

export function formatUsageResetTime(
  value: string | number | undefined,
): string | null {
  if (value === undefined) {
    return null;
  }

  if (typeof value === "string") {
    const trimmed = value.trim();
    const match = trimmed.match(/^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2}:\d{2})/);
    if (match) {
      return `${match[1]} ${match[2]}`;
    }
    if (!trimmed) {
      return null;
    }
    return trimmed;
  }

  const timestampMs = value > 1_000_000_000_000 ? value : value * 1000;
  const date = new Date(timestampMs);
  if (Number.isNaN(date.getTime())) {
    return String(value);
  }

  const pad = (part: number) => part.toString().padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(
    date.getDate(),
  )} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(
    date.getSeconds(),
  )}`;
}

function formatUsed(
  data: UsageData,
  labels: UsageSummaryLabels,
): string | null {
  if (!isNumber(data.used)) {
    return null;
  }

  if (isNumber(data.total) && data.total > 0) {
    const usedPercent = (data.used / data.total) * 100;

    if (data.unit === "%" && data.total === 100) {
      return `${labels.used} ${formatValue(data.used, "%")}`;
    }

    return `${labels.used} ${formatNumber(usedPercent)}%`;
  }

  return `${labels.used} ${formatValue(data.used, data.unit)}`;
}

export function formatUsageDataSummary(
  data: UsageData,
  labels: UsageSummaryLabels,
): string {
  const planPrefix = data.planName ? `[${data.planName}] ` : "";

  if (data.isValid === false) {
    return `${planPrefix}${data.invalidMessage || labels.invalid}`;
  }

  const windowRemainingQuota = getWindowRemainingQuota(data);
  const weeklyRemainingQuota = getWeeklyRemainingQuota(data);
  const genericRemaining =
    windowRemainingQuota === undefined && isNumber(data.remaining)
      ? data.remaining
      : undefined;
  const formattedWindowEndsAt = formatUsageResetTime(data.windowEndsAt);
  const formattedCycleEndsAt = formatUsageResetTime(data.cycleEndsAt);

  const parts = [
    formatUsed(data, labels),
    windowRemainingQuota !== undefined
      ? `${labels.fiveHourRemaining || labels.remaining} ${formatValue(windowRemainingQuota, data.unit)}`
      : null,
    genericRemaining !== undefined
      ? `${labels.remaining} ${formatValue(genericRemaining, data.unit)}`
      : null,
    weeklyRemainingQuota !== undefined
      ? `${labels.weeklyRemainingQuota || "Weekly remaining:"} ${formatValue(weeklyRemainingQuota, data.unit)}`
      : null,
    formattedWindowEndsAt
      ? `${labels.windowEndsAt || "5h reset:"} ${formattedWindowEndsAt}`
      : null,
    formattedCycleEndsAt
      ? `${labels.cycleEndsAt || "7d reset:"} ${formattedCycleEndsAt}`
      : null,
    data.extra || null,
  ].filter((part): part is string => Boolean(part));

  return `${planPrefix}${parts.join(" / ") || labels.invalid}`;
}
