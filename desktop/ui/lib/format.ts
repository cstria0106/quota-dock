export function providerMeta(source: string, plan?: string | null): string {
  return [source, plan].filter(Boolean).join(" · ");
}

export function boundedPercent(value: number): number {
  return Math.max(0, Math.min(100, value));
}
