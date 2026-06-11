export function Spinner() {
  return (
    <span className="size-6 animate-spin rounded-full border-2 border-muted border-t-emerald-700" />
  );
}

export function ErrorNotice({ message }: { message?: string | null }) {
  if (!message) {
    return null;
  }

  return (
    <div className="w-full rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-left text-sm text-red-800">
      {message}
    </div>
  );
}

export function EmptyState({ label }: { label: string }) {
  return (
    <div className="rounded-lg border border-dashed px-4 py-6 text-center text-sm text-muted-foreground">
      {label}
    </div>
  );
}
