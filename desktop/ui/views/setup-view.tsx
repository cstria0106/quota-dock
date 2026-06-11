import {
  BadgeCheck,
  CircleCheck,
  Cpu,
  Download,
  RadioTower,
  RefreshCw,
  SearchCheck,
  Send,
  TriangleAlert,
  Usb,
  Wifi,
  X,
  type LucideIcon,
} from "lucide-react";
import * as React from "react";

import { ActivityLog } from "@/components/activity-log";
import { AppHeader } from "@/components/app-header";
import { ErrorNotice, Spinner } from "@/components/feedback";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { TFunction } from "@/lib/i18n";
import { useSettings } from "@/lib/settings";
import { cn } from "@/lib/utils";
import type {
  AppSnapshot,
  RunCommand,
  SetupSnapshot,
  SetupStage,
} from "@/types";

export function SetupView({
  commandError,
  flashConfirmOpen,
  onFlashConfirmOpenChange,
  onRunCommand,
  onSubmitWifi,
  onUpdatePassword,
  onUpdateSsid,
  snapshot,
  wifiPassword,
  wifiSsid,
}: {
  commandError: string | null;
  flashConfirmOpen: boolean;
  onFlashConfirmOpenChange: (open: boolean) => void;
  onRunCommand: RunCommand;
  onSubmitWifi: () => void;
  onUpdatePassword: (value: string) => void;
  onUpdateSsid: (value: string) => void;
  snapshot: AppSnapshot;
  wifiPassword: string;
  wifiSsid: string;
}) {
  const { advanced, t } = useSettings();
  const setup = snapshot.setup;
  const StageIcon = setupIcon(setup.stage);
  const subtitle =
    advanced && setup.port
      ? t("setup.subtitle.port", { port: setup.port })
      : setup.port
        ? t("setup.subtitle")
        : t("setup.subtitle.waiting");

  return (
    <main className="h-full overflow-y-auto bg-background px-4 py-5 text-foreground sm:px-7">
      <div className="mx-auto flex min-h-full w-full max-w-6xl flex-col gap-5">
        <AppHeader
          subtitle={subtitle}
          actions={
            setup.canCancel ? (
              <Button
                type="button"
                variant="ghost"
                onClick={() => void onRunCommand("cancel_setup")}
              >
                <X />
                {t("setup.close")}
              </Button>
            ) : null
          }
        />

        <SetupSteps stage={setup.stage} t={t} />

        <section className="mx-auto grid w-full max-w-3xl flex-1 place-items-center py-3 text-center">
          <div className="grid w-full justify-items-center gap-4">
            <div
              className={cn(
                "grid size-24 place-items-center rounded-lg bg-emerald-50 text-emerald-700 ring-1 ring-emerald-900/10",
                setup.busy && "animate-pulse",
              )}
            >
              <StageIcon className="size-11" />
            </div>
            <div className="grid max-w-2xl gap-2">
              <h1 className="text-2xl font-semibold tracking-normal sm:text-3xl">
                {setup.headline}
              </h1>
              <p className="text-balance text-sm leading-6 text-muted-foreground sm:text-base">
                {setup.detail}
              </p>
            </div>
            <ErrorNotice message={setup.lastError ?? commandError} />
            <SetupStageBody
              advanced={advanced}
              onFlashConfirmOpenChange={onFlashConfirmOpenChange}
              onRunCommand={onRunCommand}
              onSubmitWifi={onSubmitWifi}
              onUpdatePassword={onUpdatePassword}
              onUpdateSsid={onUpdateSsid}
              setup={setup}
              t={t}
              wifiPassword={wifiPassword}
              wifiSsid={wifiSsid}
            />
          </div>
        </section>

        <ActivityLog lines={snapshot.log} />
      </div>

      <Dialog open={flashConfirmOpen} onOpenChange={onFlashConfirmOpenChange}>
        <DialogContent className="rounded-lg sm:max-w-md">
          <DialogHeader>
            <div className="mb-1 grid size-11 place-items-center rounded-lg bg-red-50 text-red-700">
              <TriangleAlert className="size-5" />
            </div>
            <DialogTitle>{t("setup.flash.title")}</DialogTitle>
            <DialogDescription>{t("setup.flash.desc")}</DialogDescription>
          </DialogHeader>
          <ul className="grid gap-2 text-sm text-muted-foreground">
            <li>{t("setup.flash.note1")}</li>
            <li>{t("setup.flash.note2")}</li>
            <li>{t("setup.flash.note3")}</li>
          </ul>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onFlashConfirmOpenChange(false)}
            >
              {t("setup.flash.cancel")}
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                onFlashConfirmOpenChange(false);
                void onRunCommand("confirm_flash_firmware");
              }}
            >
              <Download />
              {t("setup.flash.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </main>
  );
}

function SetupStageBody({
  advanced,
  onFlashConfirmOpenChange,
  onRunCommand,
  onSubmitWifi,
  onUpdatePassword,
  onUpdateSsid,
  setup,
  t,
  wifiPassword,
  wifiSsid,
}: {
  advanced: boolean;
  onFlashConfirmOpenChange: (open: boolean) => void;
  onRunCommand: RunCommand;
  onSubmitWifi: () => void;
  onUpdatePassword: (value: string) => void;
  onUpdateSsid: (value: string) => void;
  setup: SetupSnapshot;
  t: TFunction;
  wifiPassword: string;
  wifiSsid: string;
}) {
  if (setup.stage === "waiting_for_board" || setup.stage === "checking_firmware") {
    return (
      <div className="flex flex-wrap items-center justify-center gap-3">
        <Spinner />
        <Button
          type="button"
          variant="secondary"
          disabled={setup.busy}
          onClick={() => void onRunCommand("scan_usb")}
        >
          <RefreshCw />
          {t("setup.rescan")}
        </Button>
      </div>
    );
  }

  if (setup.stage === "needs_flash" || setup.stage === "flashing") {
    return (
      <div className="grid w-full gap-4">
        {advanced ? (
          <div className="grid overflow-hidden rounded-lg border bg-card text-left sm:grid-cols-4">
            <FirmwareCell label={t("setup.firmware.app")} value={`${setup.firmware.appKb} KB`} />
            <FirmwareCell
              label={t("setup.firmware.bootloader")}
              value={`${setup.firmware.bootloaderKb} KB`}
            />
            <FirmwareCell
              label={t("setup.firmware.partition")}
              value={`${setup.firmware.partitionTableKb} KB`}
            />
            <FirmwareCell label={t("setup.firmware.offset")} value={setup.firmware.offset} />
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("setup.firmware.ready")}
          </p>
        )}
        <div className="flex flex-wrap items-center justify-center gap-3">
          <Button
            type="button"
            variant="destructive"
            disabled={setup.stage === "flashing"}
            onClick={() => onFlashConfirmOpenChange(true)}
          >
            <Download />
            {t("setup.firmware.install")}
          </Button>
          {setup.stage === "flashing" ? <Spinner /> : null}
        </div>
      </div>
    );
  }

  if (
    setup.stage === "wifi" ||
    setup.stage === "sending_wifi" ||
    setup.stage === "connecting_wifi" ||
    setup.stage === "verifying_connection"
  ) {
    const busy = setup.stage !== "wifi";

    return (
      <form
        className="grid w-full max-w-lg gap-3 text-left"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmitWifi();
        }}
      >
        <div className="grid gap-2">
          <Label htmlFor="wifi-ssid">{t("setup.wifi.ssid")}</Label>
          <Input
            id="wifi-ssid"
            autoComplete="off"
            disabled={busy}
            value={wifiSsid}
            onChange={(event) => onUpdateSsid(event.currentTarget.value)}
          />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="wifi-password">{t("setup.wifi.password")}</Label>
          <Input
            id="wifi-password"
            type="password"
            autoComplete="current-password"
            disabled={busy}
            value={wifiPassword}
            onChange={(event) => onUpdatePassword(event.currentTarget.value)}
          />
        </div>
        <Button type="submit" disabled={busy} className="justify-self-start">
          <Send />
          {t("setup.wifi.submit")}
        </Button>
        {busy ? (
          <div className="flex justify-center">
            <Spinner />
          </div>
        ) : null}
      </form>
    );
  }

  return <Spinner />;
}

function SetupSteps({ stage, t }: { stage: SetupStage; t: TFunction }) {
  const activeIndex = setupStepIndex(stage);
  const steps: Array<{ icon: LucideIcon; label: string; stage: SetupStage }> = [
    { stage: "waiting_for_board", label: t("setup.step.usb"), icon: Usb },
    { stage: "needs_flash", label: t("setup.step.firmware"), icon: Cpu },
    { stage: "wifi", label: t("setup.step.wifi"), icon: Wifi },
    { stage: "verifying_connection", label: t("setup.step.connect"), icon: RadioTower },
  ];

  return (
    <nav className="grid overflow-hidden rounded-lg border bg-card sm:grid-cols-4">
      {steps.map((step, index) => {
        const Icon = step.icon;
        const done = index < activeIndex;
        const active = index === activeIndex;
        return (
          <div
            key={step.stage}
            className={cn(
              "flex min-w-0 items-center justify-center gap-2 border-b px-3 py-3 text-sm font-medium text-muted-foreground last:border-b-0 sm:border-r sm:border-b-0 sm:last:border-r-0",
              done && "bg-emerald-50 text-emerald-800",
              active && "bg-emerald-100 text-emerald-900",
            )}
          >
            <Icon className="size-4 shrink-0" />
            <span className="truncate">{step.label}</span>
          </div>
        );
      })}
    </nav>
  );
}

function FirmwareCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 border-b p-3 last:border-b-0 sm:border-r sm:border-b-0 sm:last:border-r-0">
      <div className="text-xs font-medium uppercase text-muted-foreground">
        {label}
      </div>
      <div className="truncate font-semibold">{value}</div>
    </div>
  );
}

function setupStepIndex(stage: SetupStage): number {
  switch (stage) {
    case "waiting_for_board":
    case "checking_firmware":
      return 0;
    case "needs_flash":
    case "flashing":
      return 1;
    case "wifi":
    case "sending_wifi":
    case "connecting_wifi":
      return 2;
    case "verifying_connection":
    case "complete":
    case "idle":
      return 3;
  }
}

function setupIcon(stage: SetupStage): LucideIcon {
  switch (stage) {
    case "waiting_for_board":
      return Usb;
    case "checking_firmware":
      return SearchCheck;
    case "needs_flash":
      return Cpu;
    case "flashing":
      return Download;
    case "wifi":
    case "sending_wifi":
      return Wifi;
    case "connecting_wifi":
      return RadioTower;
    case "verifying_connection":
      return BadgeCheck;
    case "complete":
    case "idle":
      return CircleCheck;
  }
}
