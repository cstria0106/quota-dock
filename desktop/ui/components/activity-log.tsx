import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { useSettings } from "@/lib/settings";

export function ActivityLog({ lines }: { lines: string[] }) {
  const { advanced, t } = useSettings();
  if (!advanced) {
    return null;
  }

  const visibleLines = lines.slice(-80);

  return (
    <details className="rounded-lg border bg-card">
      <summary className="cursor-pointer px-4 py-3 text-sm font-medium">
        {t("activity.title")}
      </summary>
      <Separator />
      <ScrollArea className="h-40">
        <div className="grid gap-1 px-4 py-3">
          {visibleLines.map((line, index) => (
            <code
              key={`${index}-${line}`}
              className="truncate font-mono text-xs text-muted-foreground"
            >
              {line}
            </code>
          ))}
        </div>
      </ScrollArea>
    </details>
  );
}
