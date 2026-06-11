export type Locale = "ko" | "en";

export const LOCALES: Locale[] = ["ko", "en"];

export const LOCALE_LABELS: Record<Locale, string> = {
  ko: "한국어",
  en: "English",
};

type Catalog = Record<string, string>;

const ko: Catalog = {
  "app.title": "QuotaDock",
  "app.subtitle.idle": "기기를 기다리는 중",
  "app.advanced": "개발자 모드",
  "app.closeToTray": "닫기 버튼은 트레이로",
  "app.launchAtStartup": "PC 시작 시 자동 실행",
  "app.language": "언어",
  "app.settings": "설정",

  "dock.subtitle": "사용량 모니터",
  "dock.subtitle.advanced": "기기 주소 {url}",
  "dock.setup": "기기 설정",
  "nav.usage": "사용량",
  "nav.device": "설정",
  "nav.activity": "로그",
  "dock.usage.caption": "프로바이더별 사용 한도",
  "dock.device.caption": "표시·이미지·화면·앱 설정",
  "dock.sync": "지금 동기화",
  "dock.syncing": "동기화 중",
  "dock.status.connected": "기기 연결됨",
  "dock.status.waiting": "동기화 대기 중",
  "dock.status.error": "동기화에 실패했어요",
  "dock.status.updatedRelative": "{time} 동기화됨",
  "dock.status.never": "아직 동기화하지 않았어요",
  "dock.sync.auto": "자동 동기화",
  "dock.sync.interval": "동기화 주기",

  "dock.usage.title": "사용량",
  "dock.usage.empty": "아직 사용량 정보가 없어요",
  "dock.usage.emptyHint": "‘지금 동기화’를 눌러 시작하세요.",
  "dock.usage.updated": "{time} 기준",
  "dock.usage.waiting": "동기화 대기 중",

  "dock.providers.title": "표시할 프로바이더",
  "dock.providers.empty": "사용할 수 있는 프로바이더가 없어요",
  "dock.providers.count": "표시 개수",
  "dock.providers.image": "이미지",
  "dock.providers.accent": "강조색",
  "dock.providers.defaultColor": "기본 색상",
  "dock.providers.windows": "표시할 사용량",
  "dock.providers.noWindows": "표시할 사용량이 없어요",
  "dock.images.title": "프로바이더 이미지",
  "dock.images.empty": "이미지를 설정할 프로바이더가 없어요",
  "dock.images.none": "기본 이미지",
  "dock.images.choose": "이미지 선택",
  "dock.images.clear": "이미지 제거",

  "dock.device.title": "기기",
  "dock.device.brightness": "화면 밝기",
  "dock.device.ping": "기기 응답 확인",
  "dock.device.cycle": "다음 프로바이더 표시",

  "settings.title": "앱 설정",
  "interval.30": "30초마다",
  "interval.60": "1분마다",
  "interval.300": "5분마다",
  "interval.900": "15분마다",
  "interval.1800": "30분마다",
  "interval.3600": "1시간마다",
  "interval.custom": "{secs}초마다",

  "status.ok": "여유 있음",
  "status.warning": "주의",
  "status.critical": "한도 임박",
  "status.unknown": "상태 미상",

  "setup.subtitle": "기기 설정",
  "setup.subtitle.port": "연결 포트 {port}",
  "setup.subtitle.waiting": "USB 연결 대기 중",
  "setup.close": "닫기",
  "setup.rescan": "다시 검색",
  "setup.step.usb": "기기 연결",
  "setup.step.firmware": "설치",
  "setup.step.wifi": "와이파이",
  "setup.step.connect": "연결 확인",

  "setup.firmware.install": "기기에 설치하기",
  "setup.firmware.ready": "설치 준비가 끝났어요. 아래 버튼을 눌러 진행하세요.",
  "setup.firmware.app": "앱",
  "setup.firmware.bootloader": "부트로더",
  "setup.firmware.partition": "파티션",
  "setup.firmware.offset": "오프셋",

  "setup.wifi.ssid": "와이파이 이름",
  "setup.wifi.password": "비밀번호",
  "setup.wifi.submit": "저장하고 연결",

  "setup.flash.title": "기기에 설치할까요?",
  "setup.flash.desc": "확인하면 설치가 바로 시작됩니다.",
  "setup.flash.note1": "기기의 기존 내용이 새로 덮어써집니다.",
  "setup.flash.note2": "설치하는 동안 USB 연결을 유지하세요.",
  "setup.flash.note3": "진행 중 앱·케이블·전원을 끊지 마세요.",
  "setup.flash.cancel": "취소",
  "setup.flash.confirm": "설치",

  "activity.title": "로그",
  "common.dash": "—",
};

const en: Catalog = {
  "app.title": "QuotaDock",
  "app.subtitle.idle": "Waiting for device",
  "app.advanced": "Developer mode",
  "app.closeToTray": "Close button hides to tray",
  "app.launchAtStartup": "Launch at computer startup",
  "app.language": "Language",
  "app.settings": "Settings",

  "dock.subtitle": "Usage monitor",
  "dock.subtitle.advanced": "Device at {url}",
  "dock.setup": "Device setup",
  "nav.usage": "Usage",
  "nav.device": "Settings",
  "nav.activity": "Logs",
  "dock.usage.caption": "Limits by provider",
  "dock.device.caption": "Display, images, screen, and app",
  "dock.sync": "Sync now",
  "dock.syncing": "Syncing",
  "dock.status.connected": "Device connected",
  "dock.status.waiting": "Waiting to sync",
  "dock.status.error": "Sync failed",
  "dock.status.updatedRelative": "Synced {time}",
  "dock.status.never": "Not synced yet",
  "dock.sync.auto": "Auto sync",
  "dock.sync.interval": "Sync every",

  "dock.usage.title": "Usage",
  "dock.usage.empty": "No usage data yet",
  "dock.usage.emptyHint": "Press “Sync now” to get started.",
  "dock.usage.updated": "As of {time}",
  "dock.usage.waiting": "Waiting to sync",

  "dock.providers.title": "Providers to show",
  "dock.providers.empty": "No providers available",
  "dock.providers.count": "Display count",
  "dock.providers.image": "Image",
  "dock.providers.accent": "Accent color",
  "dock.providers.defaultColor": "Default color",
  "dock.providers.windows": "Usage to show",
  "dock.providers.noWindows": "No usage windows to show",
  "dock.images.title": "Provider images",
  "dock.images.empty": "No providers to set images for",
  "dock.images.none": "Default image",
  "dock.images.choose": "Choose image",
  "dock.images.clear": "Remove image",

  "dock.device.title": "Device",
  "dock.device.brightness": "Screen brightness",
  "dock.device.ping": "Check device response",
  "dock.device.cycle": "Show next provider",

  "settings.title": "App settings",
  "interval.30": "Every 30 seconds",
  "interval.60": "Every minute",
  "interval.300": "Every 5 minutes",
  "interval.900": "Every 15 minutes",
  "interval.1800": "Every 30 minutes",
  "interval.3600": "Every hour",
  "interval.custom": "Every {secs}s",

  "status.ok": "Healthy",
  "status.warning": "Warning",
  "status.critical": "Near limit",
  "status.unknown": "Unknown",

  "setup.subtitle": "Device setup",
  "setup.subtitle.port": "Connected on {port}",
  "setup.subtitle.waiting": "Waiting for USB",
  "setup.close": "Close",
  "setup.rescan": "Scan again",
  "setup.step.usb": "Connect",
  "setup.step.firmware": "Install",
  "setup.step.wifi": "Wi-Fi",
  "setup.step.connect": "Verify",

  "setup.firmware.install": "Install to device",
  "setup.firmware.ready": "Ready to install. Press the button below to continue.",
  "setup.firmware.app": "App",
  "setup.firmware.bootloader": "Bootloader",
  "setup.firmware.partition": "Partition",
  "setup.firmware.offset": "Offset",

  "setup.wifi.ssid": "Wi-Fi name",
  "setup.wifi.password": "Password",
  "setup.wifi.submit": "Save and connect",

  "setup.flash.title": "Install to the device?",
  "setup.flash.desc": "Installation starts as soon as you confirm.",
  "setup.flash.note1": "The device's existing contents will be overwritten.",
  "setup.flash.note2": "Keep the USB connected during installation.",
  "setup.flash.note3": "Don't disconnect the app, cable, or power while it runs.",
  "setup.flash.cancel": "Cancel",
  "setup.flash.confirm": "Install",

  "activity.title": "Logs",
  "common.dash": "—",
};

const catalogs: Record<Locale, Catalog> = { ko, en };

export type TFunction = (
  key: keyof typeof ko,
  vars?: Record<string, string | number>,
) => string;

export function createTranslator(locale: Locale): TFunction {
  const catalog = catalogs[locale];
  return (key, vars) => {
    const template = catalog[key] ?? ko[key] ?? String(key);
    if (!vars) {
      return template;
    }
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in vars ? String(vars[name]) : match,
    );
  };
}

export function detectLocale(): Locale {
  const lang =
    typeof navigator !== "undefined" ? navigator.language.toLowerCase() : "ko";
  return lang.startsWith("ko") ? "ko" : "en";
}
