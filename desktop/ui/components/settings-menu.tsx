import { Settings2 } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { LOCALES, LOCALE_LABELS, type Locale } from "@/lib/i18n";
import { useSettings } from "@/lib/settings";

export function SettingsMenu({ trigger }: { trigger?: ReactNode }) {
  const { advanced, setAdvanced, locale, setLocale, t } = useSettings();

  return (
    <Popover>
      <PopoverTrigger asChild>
        {trigger ?? (
          <Button type="button" variant="ghost" size="icon" title={t("app.settings")}>
            <Settings2 />
          </Button>
        )}
      </PopoverTrigger>
      <PopoverContent>
        <div className="grid gap-4">
          <div className="grid gap-1.5">
            <Label className="text-muted-foreground">{t("app.language")}</Label>
            <Select
              value={locale}
              onValueChange={(value) => setLocale(value as Locale)}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {LOCALES.map((value) => (
                  <SelectItem key={value} value={value}>
                    {LOCALE_LABELS[value]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <label className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">{t("app.advanced")}</span>
            <Switch checked={advanced} onCheckedChange={setAdvanced} />
          </label>
        </div>
      </PopoverContent>
    </Popover>
  );
}
