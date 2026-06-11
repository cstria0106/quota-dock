import {
  CircleDot,
  ImagePlus,
  RefreshCw,
  Repeat,
  Sun,
  Trash2,
  Wrench,
} from "lucide-react";
import * as React from "react";

import { ActivityLog } from "@/components/activity-log";
import { AppHeader } from "@/components/app-header";
import { EmptyState, ErrorNotice } from "@/components/feedback";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { boundedPercent, providerMeta } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
  AppSnapshot,
  DockSnapshot,
  ImageOptionSnapshot,
  ProviderOptionSnapshot,
  RunCommand,
  UsageProvider,
  UsageSnapshot,
} from "@/types";

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
  const dock = snapshot.dock;

  return (
    <main className="min-h-screen bg-background px-4 py-5 text-foreground sm:px-7">
      <div className="mx-auto flex w-full max-w-6xl flex-col gap-5">
        <AppHeader
          subtitle={dock.deviceUrl || "IP 미저장"}
          actions={
            <div className="flex flex-wrap justify-end gap-2">
              <Button type="button" variant="secondary" onClick={onPrepareSetup}>
                <Wrench />
                초기 셋업
              </Button>
              <Button
                type="button"
                disabled={dock.syncRunning}
                onClick={() => void onRunCommand("sync_now")}
              >
                <RefreshCw className={cn(dock.syncRunning && "animate-spin")} />
                {dock.syncRunning ? "동기화 중" : "동기화"}
              </Button>
            </div>
          }
        />

        <DockStatus dock={dock} onRunCommand={onRunCommand} />
        <ErrorNotice message={dock.saveError ?? commandError} />

        <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]">
          <Card className="rounded-lg">
            <CardHeader className="border-b">
              <CardTitle>Usage</CardTitle>
              <CardDescription>
                {dock.usageSnapshot?.updated_at ?? "대기 중"}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <UsageList snapshot={dock.usageSnapshot} />
            </CardContent>
          </Card>

          <aside className="grid content-start gap-4">
            <ProvidersPanel
              onRunCommand={onRunCommand}
              providers={dock.availableProviders}
            />
            <ImagesPanel images={dock.imageOptions} onRunCommand={onRunCommand} />
            <DevicePanel
              brightnessDraft={brightnessDraft}
              dock={dock}
              onBrightnessDraft={onBrightnessDraft}
              onRunCommand={onRunCommand}
            />
          </aside>
        </section>

        <ActivityLog lines={snapshot.log} />
      </div>
    </main>
  );
}

function DockStatus({
  dock,
  onRunCommand,
}: {
  dock: DockSnapshot;
  onRunCommand: RunCommand;
}) {
  const status =
    dock.lastSyncOk === false ? "error" : dock.status?.connected ? "ok" : "muted";
  const message =
    dock.lastSyncMessage || (dock.status?.connected ? "연결됨" : "동기화 대기 중");

  return (
    <Card
      className={cn(
        "rounded-lg border-l-4",
        status === "ok" && "border-l-emerald-600",
        status === "error" && "border-l-red-600",
        status === "muted" && "border-l-muted-foreground",
      )}
    >
      <CardContent className="grid gap-4 py-4 md:grid-cols-[minmax(0,1fr)_auto_170px] md:items-center">
        <div className="min-w-0">
          <div className="truncate font-medium">{message}</div>
          <div className="truncate text-sm text-muted-foreground">
            {dock.status?.ip ?? dock.deviceUrl ?? "-"}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Switch
            checked={dock.syncEnabled}
            onCheckedChange={(enabled) => {
              void onRunCommand("set_sync_enabled", { enabled });
            }}
          />
          <span className="text-sm font-medium">Sync</span>
        </div>
        <Label className="grid gap-1 text-sm">
          <span className="text-muted-foreground">Interval</span>
          <Input
            key={dock.syncIntervalSecs}
            type="number"
            min={60}
            max={3600}
            step={1}
            defaultValue={dock.syncIntervalSecs}
            onBlur={(event) => {
              const intervalSecs = Number(event.currentTarget.value);
              if (Number.isFinite(intervalSecs)) {
                void onRunCommand("set_sync_interval", { intervalSecs });
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.currentTarget.blur();
              }
            }}
          />
        </Label>
      </CardContent>
    </Card>
  );
}

function UsageList({ snapshot }: { snapshot?: UsageSnapshot | null }) {
  if (!snapshot || snapshot.providers.length === 0) {
    return <EmptyState label="사용량 데이터 없음" />;
  }

  return (
    <div className="grid gap-4">
      {snapshot.providers.map((provider, index) => (
        <UsageProviderItem
          key={provider.id}
          provider={provider}
          showSeparator={index > 0}
        />
      ))}
    </div>
  );
}

function UsageProviderItem({
  provider,
  showSeparator,
}: {
  provider: UsageProvider;
  showSeparator: boolean;
}) {
  return (
    <div className="grid gap-3">
      {showSeparator ? <Separator /> : null}
      <div className="flex min-w-0 flex-wrap items-baseline justify-between gap-2">
        <div className="min-w-0">
          <h3 className="truncate font-medium">{provider.label}</h3>
          <p className="truncate text-sm text-muted-foreground">
            {providerMeta(provider.source, provider.plan)}
          </p>
        </div>
        {provider.account ? (
          <Badge variant="secondary" className="max-w-40 truncate">
            {provider.account}
          </Badge>
        ) : null}
      </div>
      {provider.windows.map((window) => {
        const percent = boundedPercent(window.used_percent);
        return (
          <div key={`${provider.id}-${window.kind}`} className="grid gap-2">
            <div className="flex items-center justify-between gap-3 text-sm">
              <span className="truncate font-medium">{window.label}</span>
              <span className="shrink-0 text-muted-foreground">
                {window.status}
              </span>
            </div>
            <Progress value={percent} />
          </div>
        );
      })}
    </div>
  );
}

function ProvidersPanel({
  onRunCommand,
  providers,
}: {
  onRunCommand: RunCommand;
  providers: ProviderOptionSnapshot[];
}) {
  return (
    <Card className="rounded-lg">
      <CardHeader className="border-b">
        <CardTitle>Providers</CardTitle>
      </CardHeader>
      <CardContent>
        {providers.length === 0 ? (
          <EmptyState label="사용 가능한 provider 없음" />
        ) : (
          <div className="grid gap-3">
            {providers.map((provider, index) => (
              <React.Fragment key={provider.id}>
                {index > 0 ? <Separator /> : null}
                <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3">
                  <div className="min-w-0">
                    <div className="truncate font-medium">{provider.label}</div>
                    <div className="truncate text-sm text-muted-foreground">
                      {providerMeta(provider.source, provider.plan)}
                    </div>
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
              </React.Fragment>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ImagesPanel({
  images,
  onRunCommand,
}: {
  images: ImageOptionSnapshot[];
  onRunCommand: RunCommand;
}) {
  return (
    <Card className="rounded-lg">
      <CardHeader className="border-b">
        <CardTitle>Images</CardTitle>
      </CardHeader>
      <CardContent>
        {images.length === 0 ? (
          <EmptyState label="이미지 옵션 없음" />
        ) : (
          <div className="grid gap-3">
            {images.map((image, index) => (
              <React.Fragment key={image.id}>
                {index > 0 ? <Separator /> : null}
                <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-2">
                  <div className="min-w-0">
                    <div className="truncate font-medium">{image.label}</div>
                    <div className="truncate text-sm text-muted-foreground">
                      {image.path ?? "-"}
                    </div>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    disabled={image.validating}
                    title="Choose"
                    onClick={() => {
                      void onRunCommand("choose_provider_image", {
                        providerId: image.id,
                      });
                    }}
                  >
                    <ImagePlus />
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    disabled={!image.path}
                    title="Clear"
                    onClick={() => {
                      void onRunCommand("clear_provider_image", {
                        providerId: image.id,
                      });
                    }}
                  >
                    <Trash2 />
                  </Button>
                </div>
              </React.Fragment>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function DevicePanel({
  brightnessDraft,
  dock,
  onBrightnessDraft,
  onRunCommand,
}: {
  brightnessDraft: number;
  dock: DockSnapshot;
  onBrightnessDraft: (value: number) => void;
  onRunCommand: RunCommand;
}) {
  return (
    <Card className="rounded-lg">
      <CardHeader className="border-b">
        <CardTitle>Device</CardTitle>
      </CardHeader>
      <CardContent className="grid gap-4">
        <div className="grid gap-3">
          <div className="flex items-center justify-between gap-3">
            <Label>Brightness</Label>
            <Badge variant="secondary">{brightnessDraft}</Badge>
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
            variant="secondary"
            disabled={Boolean(dock.commandRunning)}
            onClick={() =>
              void onRunCommand("set_brightness", { value: brightnessDraft })
            }
          >
            <Sun />
            적용
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon"
            disabled={Boolean(dock.commandRunning)}
            title="Ping"
            onClick={() => void onRunCommand("ping")}
          >
            <CircleDot />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon"
            disabled={Boolean(dock.commandRunning)}
            title="Cycle provider"
            onClick={() => void onRunCommand("cycle_provider")}
          >
            <Repeat />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
