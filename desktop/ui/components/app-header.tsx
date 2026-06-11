import { MonitorDot } from "lucide-react";
import type { ReactNode } from "react";

export function AppHeader({
  actions,
  subtitle,
}: {
  actions: ReactNode;
  subtitle: string;
}) {
  return (
    <header className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid size-11 shrink-0 place-items-center rounded-lg bg-emerald-50 text-emerald-700 ring-1 ring-emerald-900/10">
          <MonitorDot className="size-5" />
        </span>
        <div className="min-w-0">
          <h1 className="text-xl font-semibold tracking-normal">QuotaDock</h1>
          <p className="truncate text-sm text-muted-foreground">{subtitle}</p>
        </div>
      </div>
      {actions}
    </header>
  );
}
