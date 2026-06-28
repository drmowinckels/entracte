import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_LOCALE, isLocale, resolveLocale } from "./registry";
import type { Locale } from "./registry";
import { makeT } from "./translate";
import type { TFunc } from "./translate";

const LANG_KEY = "entracte-lang";

interface LangContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: TFunc;
}

const LangContext = createContext<LangContextValue | null>(null);

function readSaved(): Locale | null {
  try {
    const saved = localStorage.getItem(LANG_KEY);
    return saved && isLocale(saved) ? saved : null;
  } catch {
    // storage unavailable (private mode / disabled) — fall through
    return null;
  }
}

export function LangProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<Locale>(
    () => readSaved() ?? DEFAULT_LOCALE,
  );

  // A saved choice wins; otherwise resolve from the OS locale. The WebView's
  // navigator/Intl report en-US regardless of the OS region in this
  // non-localised app, so the backend's `get_locale` is the only reliable
  // source. Runs once, after the first paint (which uses the saved value or
  // English) — so a missing backend degrades to English rather than blocking.
  useEffect(() => {
    if (readSaved()) return;
    let cancelled = false;
    void (async () => {
      try {
        const tag = await invoke<string>("get_locale");
        if (!cancelled && tag) setLocale(resolveLocale(tag));
      } catch (e) {
        console.error("get_locale failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
    try {
      localStorage.setItem(LANG_KEY, locale);
    } catch {
      // storage unavailable — degrades gracefully
    }
  }, [locale]);

  const t = useMemo(() => makeT(locale), [locale]);
  const value = useMemo<LangContextValue>(
    () => ({ locale, setLocale, t }),
    [locale, t],
  );

  return <LangContext.Provider value={value}>{children}</LangContext.Provider>;
}

// Outside a provider, resolve against the default locale instead of throwing,
// so a component can render in isolation (e.g. unit tests) without ceremony.
function useLangContext(): LangContextValue {
  const ctx = useContext(LangContext);
  const fallback = useMemo<LangContextValue>(
    () => ({
      locale: DEFAULT_LOCALE,
      setLocale: () => {},
      t: makeT(DEFAULT_LOCALE),
    }),
    [],
  );
  return ctx ?? fallback;
}

export function useLang(): LangContextValue {
  return useLangContext();
}

export function useT(): TFunc {
  return useLangContext().t;
}

export function useLocale(): [Locale, (locale: Locale) => void] {
  const { locale, setLocale } = useLangContext();
  return [locale, setLocale];
}
