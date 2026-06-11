export type AppView = "setup" | "dock";

export type SetupStage =
  | "idle"
  | "waiting_for_board"
  | "checking_firmware"
  | "needs_flash"
  | "flashing"
  | "wifi"
  | "sending_wifi"
  | "connecting_wifi"
  | "verifying_connection"
  | "complete";

export type CommandName =
  | "get_app_snapshot"
  | "scan_usb"
  | "prepare_initial_setup"
  | "confirm_flash_firmware"
  | "send_wifi_credentials"
  | "cancel_setup"
  | "sync_now"
  | "set_sync_enabled"
  | "set_sync_interval"
  | "set_device_language"
  | "set_provider_enabled"
  | "set_provider_window_slot"
  | "add_provider_window"
  | "remove_provider_window"
  | "set_provider_image_visible"
  | "set_provider_accent_color"
  | "reset_provider_accent_color"
  | "choose_provider_image"
  | "clear_provider_image"
  | "set_brightness"
  | "set_close_to_tray"
  | "set_launch_at_startup"
  | "ping"
  | "cycle_provider";

export type CommandArgs = Record<string, boolean | number | string>;

export type RunCommand = (
  name: CommandName,
  args?: CommandArgs,
) => Promise<void>;

export interface StatusResponse {
  mode: string;
  connected: boolean;
  ip?: string | null;
  event?: string | null;
  heapFree: number;
  heapInternalFree: number;
  heapMinFree: number;
}

export interface FirmwareSnapshot {
  appKb: number;
  bootloaderKb: number;
  partitionTableKb: number;
  offset: string;
}

export interface SetupSnapshot {
  active: boolean;
  stage: SetupStage;
  port?: string | null;
  headline: string;
  detail: string;
  lastError?: string | null;
  wifiSsid: string;
  canCancel: boolean;
  busy: boolean;
  firmware: FirmwareSnapshot;
  status?: StatusResponse | null;
}

export interface ProviderOptionSnapshot {
  id: string;
  label: string;
  source: string;
  plan?: string | null;
  enabled: boolean;
  usageWindowLimit: number;
  showImage: boolean;
  accentColor?: string | null;
  imagePath?: string | null;
  validatingImage: boolean;
  windows: ProviderWindowOptionSnapshot[];
}

export interface ProviderWindowOptionSnapshot {
  kind: string;
  label: string;
  used_percent: number;
  status: string;
  enabled: boolean;
}

export interface UsageWindow {
  kind: string;
  label: string;
  used_percent: number;
  resets_at?: string | null;
  resets_at_unix?: number | null;
  status: string;
}

export interface UsageProvider {
  id: string;
  label: string;
  theme_color?: string | null;
  source: string;
  account?: string | null;
  plan?: string | null;
  windows: UsageWindow[];
}

export interface UsageSnapshot {
  providers: UsageProvider[];
  updated_at: string;
  updated_at_unix: number;
}

export interface ImageOptionSnapshot {
  id: string;
  label: string;
  path?: string | null;
  validating: boolean;
}

export interface DockSnapshot {
  deviceUrl: string;
  syncEnabled: boolean;
  syncIntervalSecs: number;
  syncRunning: boolean;
  lastSyncOk?: boolean | null;
  lastSyncMessage: string;
  status?: StatusResponse | null;
  availableProviders: ProviderOptionSnapshot[];
  usageSnapshot?: UsageSnapshot | null;
  imageOptions: ImageOptionSnapshot[];
  brightness: number;
  closeToTray: boolean;
  launchAtStartup: boolean;
  commandRunning?: string | null;
  saveError?: string | null;
}

export interface AppSnapshot {
  view: AppView;
  setup: SetupSnapshot;
  dock: DockSnapshot;
  log: string[];
}
