import * as React from "react";

import {
  createTranslator,
  detectLocale,
  type Locale,
  type TFunction,
} from "@/lib/i18n";

const ADVANCED_KEY = "quotadock.advanced";
const LOCALE_KEY = "quotadock.locale";

interface SettingsValue {
  advanced: boolean;
  setAdvanced: (value: boolean) => void;
  locale: Locale;
  setLocale: (value: Locale) => void;
  t: TFunction;
}

const SettingsContext = React.createContext<SettingsValue | null>(null);

function readAdvanced(): boolean {
  if (typeof localStorage === "undefined") {
    return false;
  }
  return localStorage.getItem(ADVANCED_KEY) === "true";
}

function readLocale(): Locale {
  if (typeof localStorage === "undefined") {
    return detectLocale();
  }
  const stored = localStorage.getItem(LOCALE_KEY);
  return stored === "ko" || stored === "en" ? stored : detectLocale();
}

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [advanced, setAdvancedState] = React.useState<boolean>(readAdvanced);
  const [locale, setLocaleState] = React.useState<Locale>(readLocale);

  const setAdvanced = React.useCallback((value: boolean) => {
    setAdvancedState(value);
    localStorage.setItem(ADVANCED_KEY, String(value));
  }, []);

  const setLocale = React.useCallback((value: Locale) => {
    setLocaleState(value);
    localStorage.setItem(LOCALE_KEY, value);
  }, []);

  const value = React.useMemo<SettingsValue>(
    () => ({
      advanced,
      setAdvanced,
      locale,
      setLocale,
      t: createTranslator(locale),
    }),
    [advanced, setAdvanced, locale, setLocale],
  );

  return (
    <SettingsContext.Provider value={value}>
      {children}
    </SettingsContext.Provider>
  );
}

export function useSettings(): SettingsValue {
  const value = React.useContext(SettingsContext);
  if (!value) {
    throw new Error("useSettings must be used within SettingsProvider");
  }
  return value;
}

export function useT(): TFunction {
  return useSettings().t;
}
