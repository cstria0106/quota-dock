import {
  CircleDot,
  Gauge,
  ImagePlus,
  MonitorDot,
  Palette,
  Plus,
  RefreshCw,
  Repeat,
  RotateCcw,
  ScrollText,
  SlidersHorizontal,
  Trash2,
  Wrench,
  X,
  type LucideIcon,
} from "lucide-react";
import * as React from "react";

import { invoke } from "@tauri-apps/api/core";

import { ActivityLog } from "@/components/activity-log";
import { EmptyState, ErrorNotice } from "@/components/feedback";
import { StatusPill } from "@/components/status-pill";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import {
  INTERVAL_PRESETS,
  boundedPercent,
  intervalLabel,
  relativeTime,
} from "@/lib/format";
import {
  LOCALES,
  LOCALE_LABELS,
  type Locale,
  type TFunction,
} from "@/lib/i18n";
import { useSettings } from "@/lib/settings";
import { cn } from "@/lib/utils";
import type {
  AppSnapshot,
  DockSnapshot,
  ProviderOptionSnapshot,
  ProviderWindowOptionSnapshot,
  RunCommand,
  UsageProvider,
  UsageSnapshot,
} from "@/types";

type Section = "usage" | "device" | "activity";

export function DockView({
  brightnessDraft,
  commandError,
  onBrightnessDraft,
  onPrepareSetup,
  onRunCommand,
  snapshot,
}: {
  brightnessDraft: number;
  commandError: string | null;
  onBrightnessDraft: (value: number) => void;
  onPrepareSetup: () => void;
  onRunCommand: RunCommand;
  snapshot: AppSnapshot;
}) {
  const { advanced, locale, t } = useSettings();
  const dock = snapshot.dock;
  const [section, setSection] = React.useState<Section>("usage");
  const active: Section =
    section === "activity" && !advanced ? "usage" : section;

  const title =
    active === "usage"
      ? t("nav.usage")
      : active === "device"
        ? t("nav.device")
        : t("nav.activity");
  const caption =
    active === "usage"
      ? t("dock.usage.caption")
      : active === "device"
        ? t("dock.device.caption")
        : null;

  return (
    <div className="flex h-full overflow-hidden">
      <Sidebar
        active={active}
        advanced={advanced}
        dock={dock}
        locale={locale}
        onNavigate={setSection}
        onPrepareSetup={onPrepareSetup}
        t={t}
      />

      <div className="flex min-w-0 flex-1 flex-col bg-background">
        <header className="flex shrink-0 items-center justify-between gap-4 border-b bg-card px-6 py-3.5">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold tracking-tight">
              {title}
            </h2>
            {caption ? (
              <p className="truncate text-xs text-muted-foreground">{caption}</p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <div className="hidden items-center gap-2 sm:flex">
              <Switch
                checked={dock.syncEnabled}
                onCheckedChange={(enabled) =>
                  void onRunCommand("set_sync_enabled", { enabled })
                }
              />
              <span className="text-sm text-muted-foreground">
                {t("dock.sync.auto")}
              </span>
            </div>
            <Select
              value={String(dock.syncIntervalSecs)}
              onValueChange={(value) =>
                void onRunCommand("set_sync_interval", {
                  intervalSecs: Number(value),
                })
              }
            >
              <SelectTrigger className="h-9 w-auto gap-2">
                <SelectValue>
                  {intervalLabel(t, dock.syncIntervalSecs)}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {INTERVAL_PRESETS.map((secs) => (
                  <SelectItem key={secs} value={String(secs)}>
                    {intervalLabel(t, secs)}
                  </SelectItem>
                ))}
                {!(INTERVAL_PRESETS as readonly number[]).includes(
                  dock.syncIntervalSecs,
                ) ? (
                  <SelectItem value={String(dock.syncIntervalSecs)}>
                    {intervalLabel(t, dock.syncIntervalSecs)}
                  </SelectItem>
                ) : null}
              </SelectContent>
            </Select>
            <Button
              type="button"
              disabled={dock.syncRunning}
              onClick={() => void onRunCommand("sync_now")}
            >
              <RefreshCw className={cn(dock.syncRunning && "animate-spin")} />
              {dock.syncRunning ? t("dock.syncing") : t("dock.sync")}
            </Button>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
            <ErrorNotice message={dock.saveError ?? commandError} />

            {active === "usage" ? (
              <UsageList advanced={advanced} snapshot={dock.usageSnapshot} t={t} />
            ) : active === "device" ? (
              <DeviceSection
                advanced={advanced}
                brightnessDraft={brightnessDraft}
                dock={dock}
                onBrightnessDraft={onBrightnessDraft}
                onRunCommand={onRunCommand}
                t={t}
              />
            ) : (
              <ActivityLog lines={snapshot.log} />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function Sidebar({
  active,
  advanced,
  dock,
  locale,
  onNavigate,
  onPrepareSetup,
  t,
}: {
  active: Section;
  advanced: boolean;
  dock: DockSnapshot;
  locale: string;
  onNavigate: (section: Section) => void;
  onPrepareSetup: () => void;
  t: TFunction;
}) {
  const connState =
    dock.lastSyncOk === false
      ? "error"
      : dock.status?.connected
        ? "ok"
        : "muted";
  const connLabel =
    connState === "error"
      ? t("dock.status.error")
      : connState === "ok"
        ? t("dock.status.connected")
        : t("dock.status.waiting");
  const synced = relativeTime(locale, dock.usageSnapshot?.updated_at_unix);
  const subline = advanced
    ? dock.status?.ip || dock.deviceUrl || t("common.dash")
    : synced
      ? t("dock.status.updatedRelative", { time: synced })
      : t("dock.status.never");

  return (
    <aside className="flex w-56 shrink-0 flex-col bg-sidebar text-sidebar-foreground">
      <div className="flex items-center gap-2.5 px-4 py-4">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-sidebar-primary/15 text-sidebar-primary">
          <MonitorDot className="size-5" />
        </span>
        <div className="text-sm font-semibold tracking-tight">
          {t("app.title")}
        </div>
      </div>

      <nav className="grid gap-1 px-3 py-2">
        <NavButton
          active={active === "usage"}
          icon={Gauge}
          label={t("nav.usage")}
          onClick={() => onNavigate("usage")}
        />
        <NavButton
          active={active === "device"}
          icon={SlidersHorizontal}
          label={t("nav.device")}
          onClick={() => onNavigate("device")}
        />
        {advanced ? (
          <NavButton
            active={active === "activity"}
            icon={ScrollText}
            label={t("nav.activity")}
            onClick={() => onNavigate("activity")}
          />
        ) : null}
      </nav>

      <div className="mt-auto grid gap-3 px-3 py-3">
        <div className="flex items-start gap-2.5 rounded-lg bg-sidebar-accent/50 px-3 py-2.5">
          <span
            className={cn(
              "mt-1 size-2 shrink-0 rounded-full",
              connState === "ok" && "bg-emerald-400",
              connState === "error" && "bg-red-400",
              connState === "muted" && "bg-sidebar-foreground/30",
            )}
          />
          <div className="min-w-0">
            <div className="truncate text-xs font-medium">{connLabel}</div>
            <div className="truncate text-xs text-sidebar-foreground/55">
              {subline}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1">
          <Button
            type="button"
            variant="ghost"
            onClick={onPrepareSetup}
            className="flex-1 justify-start text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
          >
            <Wrench />
            {t("dock.setup")}
          </Button>
        </div>
      </div>
    </aside>
  );
}

function NavButton({
  active,
  icon: Icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: LucideIcon;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors",
        active
          ? "bg-sidebar-primary text-sidebar-primary-foreground"
          : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
      )}
    >
      <Icon className="size-4 shrink-0" />
      {label}
    </button>
  );
}

function UsageList({
  advanced,
  snapshot,
  t,
}: {
  advanced: boolean;
  snapshot?: UsageSnapshot | null;
  t: TFunction;
}) {
  if (!snapshot || snapshot.providers.length === 0) {
    return (
      <EmptyState
        label={`${t("dock.usage.empty")} · ${t("dock.usage.emptyHint")}`}
      />
    );
  }

  return (
    <div className="grid gap-4">
      {snapshot.providers.map((provider) => (
        <UsageProviderCard advanced={advanced} key={provider.id} provider={provider} />
      ))}
    </div>
  );
}

function UsageProviderCard({
  advanced,
  provider,
}: {
  advanced: boolean;
  provider: UsageProvider;
}) {
  const meta = advanced
    ? [provider.source, provider.plan].filter(Boolean).join(" · ")
    : provider.plan;

  return (
    <Card className="rounded-xl">
      <CardHeader className="border-b">
        <CardTitle className="truncate">{provider.label}</CardTitle>
        {meta ? (
          <CardDescription className="truncate">{meta}</CardDescription>
        ) : null}
        {provider.account ? (
          <CardAction>
            <Badge variant="secondary" className="max-w-40 truncate">
              {provider.account}
            </Badge>
          </CardAction>
        ) : null}
      </CardHeader>
      <CardContent className="grid gap-4">
        {provider.windows.map((window) => {
          const percent = boundedPercent(window.used_percent);
          return (
            <div key={`${provider.id}-${window.kind}`} className="grid gap-2">
              <div className="flex items-center justify-between gap-3 text-sm">
                <span className="truncate font-medium">{window.label}</span>
                <div className="flex shrink-0 items-center gap-2">
                  <span className="tabular-nums text-muted-foreground">
                    {Math.round(percent)}%
                  </span>
                  <StatusPill status={window.status} />
                </div>
              </div>
              <Progress value={percent} />
            </div>
          );
        })}
      </CardContent>
    </Card>
  );
}

function DeviceSection({
  advanced,
  brightnessDraft,
  dock,
  onBrightnessDraft,
  onRunCommand,
  t,
}: {
  advanced: boolean;
  brightnessDraft: number;
  dock: DockSnapshot;
  onBrightnessDraft: (value: number) => void;
  onRunCommand: RunCommand;
  t: TFunction;
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <ProvidersPanel
        advanced={advanced}
        onRunCommand={onRunCommand}
        providers={dock.availableProviders}
        t={t}
      />
      <DevicePanel
        advanced={advanced}
        brightnessDraft={brightnessDraft}
        dock={dock}
        onBrightnessDraft={onBrightnessDraft}
        onRunCommand={onRunCommand}
        t={t}
      />
      <SettingsPanel dock={dock} onRunCommand={onRunCommand} t={t} />
    </div>
  );
}

function SettingsPanel({
  dock,
  onRunCommand,
  t,
}: {
  dock: DockSnapshot;
  onRunCommand: RunCommand;
  t: TFunction;
}) {
  const { advanced, setAdvanced, locale, setLocale } = useSettings();

  return (
    <Card className="rounded-xl lg:col-span-2">
      <CardHeader className="border-b">
        <CardTitle>{t("settings.title")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3">
          <Label>{t("app.language")}</Label>
          <Select
            value={locale}
            onValueChange={(value) => setLocale(value as Locale)}
          >
            <SelectTrigger className="w-40">
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
        <Separator />
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm font-medium">{t("app.advanced")}</span>
          <Switch checked={advanced} onCheckedChange={setAdvanced} />
        </label>
        <Separator />
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm font-medium">{t("app.closeToTray")}</span>
          <Switch
            checked={dock.closeToTray}
            onCheckedChange={(enabled) =>
              void onRunCommand("set_close_to_tray", { enabled })
            }
          />
        </label>
        <Separator />
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm font-medium">{t("app.launchAtStartup")}</span>
          <Switch
            checked={dock.launchAtStartup}
            onCheckedChange={(enabled) =>
              void onRunCommand("set_launch_at_startup", { enabled })
            }
          />
        </label>
      </CardContent>
    </Card>
  );
}

function ProvidersPanel({
  advanced,
  onRunCommand,
  providers,
  t,
}: {
  advanced: boolean;
  onRunCommand: RunCommand;
  providers: ProviderOptionSnapshot[];
  t: TFunction;
}) {
  return (
    <Card className="rounded-xl lg:col-span-2">
      <CardHeader className="border-b">
        <CardTitle>{t("dock.providers.title")}</CardTitle>
      </CardHeader>
      <CardContent>
        {providers.length === 0 ? (
          <EmptyState label={t("dock.providers.empty")} />
        ) : (
          <div className="grid gap-5">
            {providers.map((provider) => (
              <ProviderSettingsCard
                advanced={advanced}
                key={provider.id}
                onRunCommand={onRunCommand}
                provider={provider}
                t={t}
              />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ProviderSettingsCard({
  advanced,
  onRunCommand,
  provider,
  t,
}: {
  advanced: boolean;
  onRunCommand: RunCommand;
  provider: ProviderOptionSnapshot;
  t: TFunction;
}) {
  const [preview, setPreview] = React.useState<string | null>(null);
  const meta = advanced
    ? [provider.source, provider.plan].filter(Boolean).join(" · ")
    : provider.plan;
  const visibleWindows = provider.windows
    .filter((window) => window.enabled)
    .slice(0, provider.usageWindowLimit);

  React.useEffect(() => {
    let cancelled = false;
    if (!provider.imagePath || provider.validatingImage || !provider.showImage) {
      setPreview(null);
      return;
    }
    void invoke<string | null>("provider_image_preview", {
      providerId: provider.id,
    })
      .then((value) => {
        if (!cancelled) {
          setPreview(value ?? null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPreview(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [provider.id, provider.imagePath, provider.showImage, provider.validatingImage]);

  return (
    <div className="grid gap-4 rounded-lg border p-4">
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3">
        <div className="min-w-0">
          <div className="truncate font-medium">{provider.label}</div>
          {meta ? (
            <div className="truncate text-sm text-muted-foreground">{meta}</div>
          ) : null}
        </div>
        <Switch
          checked={provider.enabled}
          onCheckedChange={(enabled) => {
            void onRunCommand("set_provider_enabled", {
              providerId: provider.id,
              enabled,
            });
          }}
        />
      </div>

      {provider.enabled ? (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_18rem]">
          <QuotaPreview
            accentColor={provider.accentColor ?? "#3B82F6"}
            availableWindows={provider.windows}
            image={provider.showImage ? preview : null}
            onSlotChange={(slot, kind) => {
              void onRunCommand("set_provider_window_slot", {
                providerId: provider.id,
                slot,
                kind,
              });
            }}
            onSlotRemove={(slot) => {
              void onRunCommand("remove_provider_window", {
                providerId: provider.id,
                slot,
              });
            }}
            onWindowAdd={(kind) => {
              void onRunCommand("add_provider_window", {
                providerId: provider.id,
                kind,
              });
            }}
            t={t}
            windows={visibleWindows}
          />
          <div className="grid content-start gap-4">
            <ImageControls
              onRunCommand={onRunCommand}
              provider={provider}
              t={t}
            />
            <ColorControl
              color={provider.accentColor ?? "#3B82F6"}
              onChange={(color) => {
                void onRunCommand("set_provider_accent_color", {
                  providerId: provider.id,
                  color,
                });
              }}
              onReset={() => {
                void onRunCommand("reset_provider_accent_color", {
                  providerId: provider.id,
                });
              }}
              t={t}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}

function ImageControls({
  onRunCommand,
  provider,
  t,
}: {
  onRunCommand: RunCommand;
  provider: ProviderOptionSnapshot;
  t: TFunction;
}) {
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <Label>{t("dock.providers.image")}</Label>
        <Switch
          checked={provider.showImage}
          onCheckedChange={(visible) => {
            void onRunCommand("set_provider_image_visible", {
              providerId: provider.id,
              visible,
            });
          }}
        />
      </div>
      <div className="flex gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={provider.validatingImage}
          onClick={() => {
            void onRunCommand("choose_provider_image", {
              providerId: provider.id,
            });
          }}
        >
          <ImagePlus />
          {t("dock.images.choose")}
        </Button>
        <Button
          type="button"
          variant="outline"
          size="icon"
          disabled={!provider.imagePath}
          title={t("dock.images.clear")}
          onClick={() => {
            void onRunCommand("clear_provider_image", {
              providerId: provider.id,
            });
          }}
        >
          <Trash2 />
        </Button>
      </div>
    </div>
  );
}

function ColorControl({
  color,
  onChange,
  onReset,
  t,
}: {
  color: string;
  onChange: (color: string) => void;
  onReset: () => void;
  t: TFunction;
}) {
  return (
    <div className="grid gap-2">
      <Label>{t("dock.providers.accent")}</Label>
      <div className="flex gap-2">
        <label className="relative grid size-9 place-items-center overflow-hidden rounded-md border">
          <Palette className="pointer-events-none absolute size-4 text-muted-foreground" />
          <input
            aria-label="강조색"
            className="size-12 cursor-pointer opacity-0"
            type="color"
            value={color}
            onChange={(event) => onChange(event.currentTarget.value)}
          />
        </label>
        <Badge variant="secondary" className="min-w-24 justify-center">
          {color}
        </Badge>
        <Button
          type="button"
          variant="outline"
          size="icon"
          title={t("dock.providers.defaultColor")}
          onClick={onReset}
        >
          <RotateCcw />
        </Button>
      </div>
    </div>
  );
}

function QuotaPreview({
  accentColor,
  availableWindows,
  image,
  onSlotChange,
  onSlotRemove,
  onWindowAdd,
  t,
  windows,
}: {
  accentColor: string;
  availableWindows: ProviderWindowOptionSnapshot[];
  image: string | null;
  onSlotChange: (slot: number, kind: string) => void;
  onSlotRemove: (slot: number) => void;
  onWindowAdd: (kind: string) => void;
  t: TFunction;
  windows: ProviderWindowOptionSnapshot[];
}) {
  const panelStyle = previewTheme(accentColor);
  const addOptions = availableWindows.filter(
    (option) => !windows.some((window) => window.kind === option.kind),
  );
  if (windows.length === 0) {
    return <EmptyState label={t("dock.providers.noWindows")} />;
  }

  return (
    <div className="grid aspect-[5/3] min-h-52 content-center gap-3 rounded-lg border bg-zinc-950 p-4">
      <PreviewUsageCard
        availableWindows={availableWindows}
        image={image}
        onWindowChange={(kind) => onSlotChange(0, kind)}
        onWindowRemove={windows.length > 1 ? () => onSlotRemove(0) : undefined}
        primary
        style={panelStyle}
        window={windows[0]}
      />
      {windows.length === 1 && addOptions.length > 0 ? (
        <PreviewAddCard
          options={addOptions}
          onAdd={onWindowAdd}
          style={panelStyle}
        />
      ) : null}
      {windows.length === 2 ? (
        <div className="grid grid-cols-2 gap-3">
          <PreviewUsageCard
            availableWindows={availableWindows}
            onWindowChange={(kind) => onSlotChange(1, kind)}
            onWindowRemove={() => onSlotRemove(1)}
            style={panelStyle}
            window={windows[1]}
          />
          {addOptions.length > 0 ? (
            <PreviewAddCard
              options={addOptions}
              onAdd={onWindowAdd}
              style={panelStyle}
            />
          ) : null}
        </div>
      ) : null}
      {windows.length >= 3 ? (
        <div className="grid grid-cols-2 gap-3">
          <PreviewUsageCard
            availableWindows={availableWindows}
            onWindowChange={(kind) => onSlotChange(1, kind)}
            onWindowRemove={() => onSlotRemove(1)}
            style={panelStyle}
            window={windows[1]}
          />
          <PreviewUsageCard
            availableWindows={availableWindows}
            onWindowChange={(kind) => onSlotChange(2, kind)}
            onWindowRemove={() => onSlotRemove(2)}
            style={panelStyle}
            window={windows[2]}
          />
        </div>
      ) : null}
    </div>
  );
}

function PreviewAddCard({
  onAdd,
  options,
  style,
}: {
  onAdd: (kind: string) => void;
  options: ProviderWindowOptionSnapshot[];
  style: React.CSSProperties;
}) {
  return (
    <div
      className="grid min-h-20 place-items-center rounded-md border border-dashed border-white/20 p-3 text-white"
      style={style}
    >
      <Select onValueChange={onAdd}>
        <SelectTrigger className="size-10 rounded-full border-white/10 bg-black/25 p-0 text-white">
          <Plus className="mx-auto size-5" />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.kind} value={option.kind}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

function PreviewUsageCard({
  availableWindows,
  image,
  onWindowChange,
  onWindowRemove,
  primary = false,
  style,
  window,
}: {
  availableWindows: ProviderWindowOptionSnapshot[];
  image?: string | null;
  onWindowChange: (kind: string) => void;
  onWindowRemove?: () => void;
  primary?: boolean;
  style: React.CSSProperties;
  window: ProviderWindowOptionSnapshot;
}) {
  const percent = boundedPercent(window.used_percent);
  return (
    <div
      className={cn(
        "relative grid min-h-20 gap-3 rounded-md p-3 text-white",
        primary && image ? "grid-cols-[4.5rem_minmax(0,1fr)] items-center" : "",
      )}
      style={style}
    >
      {onWindowRemove ? (
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="absolute top-2 right-2 size-6 border-white/10 bg-black/25 text-white hover:bg-black/40 hover:text-white"
          onClick={onWindowRemove}
        >
          <X className="size-3" />
        </Button>
      ) : null}
      {primary && image ? (
        <span className="grid aspect-square place-items-center overflow-hidden rounded-md bg-black/20">
          <img alt="" className="size-full object-contain" src={image} />
        </span>
      ) : null}
      <div className="grid min-w-0 content-center gap-2">
        <div className="flex items-center justify-between gap-3">
          <span className="text-2xl font-semibold tabular-nums">
            {Math.round(percent)}%
          </span>
          <Select value={window.kind} onValueChange={onWindowChange}>
            <SelectTrigger className="h-7 w-auto min-w-20 rounded-full border-white/10 bg-black/25 px-2 py-1 text-xs text-white">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {availableWindows.map((option) => (
                <SelectItem key={option.kind} value={option.kind}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <Progress value={percent} />
      </div>
    </div>
  );
}

function previewTheme(accentColor: string): React.CSSProperties {
  return {
    background: `linear-gradient(135deg, ${accentColor}2E, #111827)`,
    borderColor: `${accentColor}44`,
  };
}

function DevicePanel({
  advanced,
  brightnessDraft,
  dock,
  onBrightnessDraft,
  onRunCommand,
  t,
}: {
  advanced: boolean;
  brightnessDraft: number;
  dock: DockSnapshot;
  onBrightnessDraft: (value: number) => void;
  onRunCommand: RunCommand;
  t: TFunction;
}) {
  const brightnessPercent = Math.round((brightnessDraft / 255) * 100);

  return (
    <Card className="rounded-xl lg:col-span-2">
      <CardHeader className="border-b">
        <CardTitle>{t("dock.device.title")}</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <div className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <Label>{t("dock.device.brightness")}</Label>
            <Badge variant="secondary">
              {advanced ? brightnessDraft : `${brightnessPercent}%`}
            </Badge>
          </div>
          <Slider
            min={0}
            max={255}
            step={1}
            value={[brightnessDraft]}
            onValueChange={(values) => {
              const [value] = values;
              if (typeof value === "number") {
                onBrightnessDraft(value);
              }
            }}
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={Boolean(dock.commandRunning)}
            onClick={() => void onRunCommand("cycle_provider")}
          >
            <Repeat />
            {t("dock.device.cycle")}
          </Button>
          {advanced ? (
            <Button
              type="button"
              variant="outline"
              size="icon"
              disabled={Boolean(dock.commandRunning)}
              title={t("dock.device.ping")}
              onClick={() => void onRunCommand("ping")}
            >
              <CircleDot />
            </Button>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}
