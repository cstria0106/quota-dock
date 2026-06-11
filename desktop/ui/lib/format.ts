import type { TFunction } from "@/lib/i18n";

export function boundedPercent(value: number): number {
  return Math.max(0, Math.min(100, value));
}

export type UsageStatus =
  | "live"
  | "estimated"
  | "error"
  | "ok"
  | "warning"
  | "critical"
  | "unknown";

export function normalizeStatus(status: string): UsageStatus {
  const value = status.trim().toLowerCase();
  if (
    value === "live" ||
    value === "estimated" ||
    value === "error" ||
    value === "ok" ||
    value === "warning" ||
    value === "critical"
  ) {
    return value;
  }
  return "unknown";
}

export function usageStatusLabel(t: TFunction, status: string): string {
  return t(`status.${normalizeStatus(status)}` as Parameters<TFunction>[0]);
}

export const INTERVAL_PRESETS = [30, 60, 300, 900, 1800, 3600] as const;

export function intervalLabel(t: TFunction, secs: number): string {
  if ((INTERVAL_PRESETS as readonly number[]).includes(secs)) {
    return t(`interval.${secs}` as Parameters<TFunction>[0]);
  }
  return t("interval.custom", { secs });
}

const RELATIVE_THRESHOLDS: Array<[limitSecs: number, unitSecs: number, unit: Intl.RelativeTimeFormatUnit]> = [
  [60, 1, "second"],
  [3600, 60, "minute"],
  [86400, 3600, "hour"],
  [Number.POSITIVE_INFINITY, 86400, "day"],
];

// Renders a parseable timestamp (ISO string or unix epoch seconds) as
// "3 minutes ago" in the active locale.
export function relativeTime(
  locale: string,
  timestamp?: string | number | null,
): string | null {
  if (timestamp === undefined || timestamp === null) {
    return null;
  }
  const parsed =
    typeof timestamp === "number" ? timestamp * 1000 : Date.parse(timestamp);
  if (Number.isNaN(parsed)) {
    return null;
  }
  const diffSecs = Math.round((parsed - Date.now()) / 1000);
  const abs = Math.abs(diffSecs);
  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  for (const [limit, unitSecs, unit] of RELATIVE_THRESHOLDS) {
    if (abs < limit) {
      return formatter.format(Math.round(diffSecs / unitSecs), unit);
    }
  }
  return formatter.format(Math.round(diffSecs / 86400), "day");
}
