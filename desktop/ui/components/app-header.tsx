import { MonitorDot } from "lucide-react";
import type { ReactNode } from "react";

import { SettingsMenu } from "@/components/settings-menu";
import { useT } from "@/lib/settings";

export function AppHeader({
  actions,
  subtitle,
}: {
  actions: ReactNode;
  subtitle: string;
}) {
  const t = useT();

  return (
    <header className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-3">
        <span className="grid size-11 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary ring-1 ring-primary/15">
          <MonitorDot className="size-5" />
        </span>
        <div className="min-w-0">
          <h1 className="text-xl font-semibold tracking-tight">
            {t("app.title")}
          </h1>
          <p className="truncate text-sm text-muted-foreground">{subtitle}</p>
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">
        {actions}
        <SettingsMenu />
      </div>
    </header>
  );
}
