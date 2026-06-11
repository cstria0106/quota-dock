import { CircleCheck, CircleSlash, TriangleAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { normalizeStatus, usageStatusLabel, type UsageStatus } from "@/lib/format";
import { useT } from "@/lib/settings";
import { cn } from "@/lib/utils";

const STYLES: Record<UsageStatus, { icon: LucideIcon; className: string }> = {
  live: {
    icon: CircleCheck,
    className: "bg-emerald-50 text-emerald-700 ring-emerald-900/10",
  },
  estimated: {
    icon: TriangleAlert,
    className: "bg-amber-50 text-amber-700 ring-amber-900/10",
  },
  error: {
    icon: TriangleAlert,
    className: "bg-red-50 text-red-700 ring-red-900/10",
  },
  ok: {
    icon: CircleCheck,
    className: "bg-emerald-50 text-emerald-700 ring-emerald-900/10",
  },
  warning: {
    icon: TriangleAlert,
    className: "bg-amber-50 text-amber-700 ring-amber-900/10",
  },
  critical: {
    icon: TriangleAlert,
    className: "bg-red-50 text-red-700 ring-red-900/10",
  },
  unknown: {
    icon: CircleSlash,
    className: "bg-muted text-muted-foreground ring-black/5",
  },
};

export function StatusPill({ status }: { status: string }) {
  const t = useT();
  const kind = normalizeStatus(status);
  const { icon: Icon, className } = STYLES[kind];

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset",
        className,
      )}
    >
      <Icon className="size-3" />
      {usageStatusLabel(t, status)}
    </span>
  );
}
