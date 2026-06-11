import type { LucideIcon } from "lucide-react";
import type * as React from "react";

export function Spinner() {
  return (
    <span className="size-6 animate-spin rounded-full border-2 border-muted border-t-primary" />
  );
}

export function ErrorNotice({ message }: { message?: string | null }) {
  if (!message) {
    return null;
  }

  return (
    <div className="w-full rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-left text-sm text-destructive">
      {message}
    </div>
  );
}

export function EmptyState({
  label,
  icon: Icon,
  action,
}: {
  label: string;
  icon?: LucideIcon;
  action?: React.ReactNode;
}) {
  return (
    <div className="grid justify-items-center gap-3 rounded-lg border border-dashed px-4 py-8 text-center">
      {Icon ? (
        <span className="grid size-10 place-items-center rounded-full bg-muted text-muted-foreground">
          <Icon className="size-5" />
        </span>
      ) : null}
      <span className="text-sm text-muted-foreground">{label}</span>
      {action}
    </div>
  );
}
