import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Link, useLocation } from "react-router";

import { useAccessI18n } from "@/lib/access-i18n";
import { localizedPath } from "@/lib/site-locale";

export type LandingLocale = "zh" | "en";

export function LandingLanguageProvider({ children }: { children: ReactNode }) {
  return <>{children}</>;
}

export function useLandingLocale() {
  return useLandingLanguage().locale;
}

export function useLandingLanguage() {
  const { i18n } = useAccessI18n();
  const location = useLocation();
  const locale = i18n.language === "en-US" ? ("en" as const) : ("zh" as const);
  return {
    locale,
    localeHref: (nextLocale: LandingLocale) => ({
      hash: location.hash,
      pathname: localizedPath(location.pathname, nextLocale),
      search: location.search,
    }),
    selectLocale: (nextLocale: LandingLocale) => {
      void i18n.changeLanguage(nextLocale === "en" ? "en-US" : "zh-CN");
    },
  };
}

export function LanguageSwitcher() {
  const { locale, localeHref, selectLocale } = useLandingLanguage();
  const [open, setOpen] = useState(false);
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  function cancelClose() {
    if (!closeTimerRef.current) return;
    clearTimeout(closeTimerRef.current);
    closeTimerRef.current = null;
  }

  function scheduleClose() {
    cancelClose();
    closeTimerRef.current = setTimeout(() => setOpen(false), 360);
  }

  useEffect(() => {
    function closeFromOutside(event: PointerEvent) {
      if (!detailsRef.current?.contains(event.target as Node)) setOpen(false);
    }
    function closeFromKeyboard(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      setOpen(false);
      detailsRef.current?.querySelector("summary")?.focus();
    }
    document.addEventListener("pointerdown", closeFromOutside);
    document.addEventListener("keydown", closeFromKeyboard);
    return () => {
      cancelClose();
      document.removeEventListener("pointerdown", closeFromOutside);
      document.removeEventListener("keydown", closeFromKeyboard);
    };
  }, []);

  return (
    <details
      ref={detailsRef}
      className="language-switcher"
      open={open}
      onMouseEnter={cancelClose}
      onMouseLeave={scheduleClose}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary
        aria-label={locale === "en" ? "Change language" : "切换语言"}
        onClick={(event) => {
          event.preventDefault();
          cancelClose();
          setOpen((current) => !current);
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c2.4 2.5 3.6 5.5 3.6 9S14.4 18.5 12 21M12 3C9.6 5.5 8.4 8.5 8.4 12S9.6 18.5 12 21" />
        </svg>
        <span>{locale === "en" ? "EN" : "中文"}</span>
      </summary>
      <div className="header-dropdown-menu language-switcher-menu">
        {(["zh", "en"] as const).map((nextLocale) => (
          <Link
            to={localeHref(nextLocale)}
            className={locale === nextLocale ? "active" : ""}
            onClick={() => {
              selectLocale(nextLocale);
              setOpen(false);
            }}
            key={nextLocale}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path d="m3 8 3 3 7-7" />
            </svg>
            <span>{nextLocale === "zh" ? "中文" : "EN"}</span>
          </Link>
        ))}
      </div>
    </details>
  );
}

export function LangText({ zh, en }: { zh: string; en: string }) {
  return <>{useLandingLocale() === "en" ? en : zh}</>;
}
