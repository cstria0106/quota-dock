import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as React from "react";

import { Spinner } from "@/components/feedback";
import { useSettings } from "@/lib/settings";
import type { AppSnapshot, CommandArgs, CommandName } from "@/types";
import { DockView } from "@/views/dock-view";
import { SetupView } from "@/views/setup-view";

export function App() {
  const { locale } = useSettings();
  const [snapshot, setSnapshot] = React.useState<AppSnapshot | null>(null);
  const [commandError, setCommandError] = React.useState<string | null>(null);
  const [flashConfirmOpen, setFlashConfirmOpen] = React.useState(false);
  const [wifiSsid, setWifiSsid] = React.useState("");
  const [wifiPassword, setWifiPassword] = React.useState("");
  const [brightnessDraft, setBrightnessDraft] = React.useState(255);
  const wifiTouchedRef = React.useRef(false);
  const brightnessTouchedRef = React.useRef(false);
  const brightnessTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const applySnapshot = React.useCallback((next: AppSnapshot) => {
    setSnapshot(next);
    if (!wifiTouchedRef.current) {
      setWifiSsid(next.setup.wifiSsid);
    }
    if (!brightnessTouchedRef.current) {
      setBrightnessDraft(next.dock.brightness);
    }
  }, []);

  const runCommand = React.useCallback(
    async (name: CommandName, args?: CommandArgs) => {
      setCommandError(null);
      try {
        const next = await invoke<AppSnapshot>(name, args);
        applySnapshot(next);
      } catch (error) {
        setCommandError(String(error));
      }
    },
    [applySnapshot],
  );

  React.useEffect(() => {
    void runCommand("set_device_language", { language: locale });
  }, [locale, runCommand]);

  React.useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<AppSnapshot>("app-snapshot", (event) => {
      if (!disposed) {
        applySnapshot(event.payload);
      }
    }).then((handler) => {
      unlisten = handler;
    });
    void runCommand("get_app_snapshot");

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [applySnapshot, runCommand]);

  React.useEffect(() => {
    if (snapshot?.setup.stage !== "needs_flash") {
      setFlashConfirmOpen(false);
    }
  }, [snapshot?.setup.stage]);

  const handleBrightnessDraft = React.useCallback(
    (value: number) => {
      brightnessTouchedRef.current = true;
      setBrightnessDraft(value);
      if (brightnessTimerRef.current) {
        clearTimeout(brightnessTimerRef.current);
      }
      brightnessTimerRef.current = setTimeout(() => {
        void runCommand("set_brightness", { value });
      }, 400);
    },
    [runCommand],
  );

  React.useEffect(
    () => () => {
      if (brightnessTimerRef.current) {
        clearTimeout(brightnessTimerRef.current);
      }
    },
    [],
  );

  if (!snapshot) {
    return (
      <main className="grid h-full place-items-center bg-background">
        <Spinner />
      </main>
    );
  }

  if (snapshot.view === "dock") {
    return (
      <DockView
        brightnessDraft={brightnessDraft}
        commandError={commandError}
        onBrightnessDraft={handleBrightnessDraft}
        onPrepareSetup={() => {
          wifiTouchedRef.current = false;
          setWifiPassword("");
          void runCommand("prepare_initial_setup");
        }}
        onRunCommand={runCommand}
        snapshot={snapshot}
      />
    );
  }

  return (
    <SetupView
      commandError={commandError}
      flashConfirmOpen={flashConfirmOpen}
      onFlashConfirmOpenChange={setFlashConfirmOpen}
      onRunCommand={runCommand}
      onSubmitWifi={() => {
        void runCommand("send_wifi_credentials", {
          ssid: wifiSsid,
          password: wifiPassword,
        });
      }}
      onUpdatePassword={setWifiPassword}
      onUpdateSsid={(value) => {
        wifiTouchedRef.current = true;
        setWifiSsid(value);
      }}
      snapshot={snapshot}
      wifiPassword={wifiPassword}
      wifiSsid={wifiSsid}
    />
  );
}
