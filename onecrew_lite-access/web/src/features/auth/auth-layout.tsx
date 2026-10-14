import type { ReactNode } from "react";

import { AuthPhotoCarousel } from "./auth-photo-carousel";
import "./auth-page.css";
import { useAccessI18n } from "@/lib/access-i18n";
import { PUBLIC_SITE_URL } from "@/lib/public-site";
import { AccessLanguageToggle } from "./access-language-toggle";

export function AuthLayout({ children }: { children: ReactNode }) {
  const { locale, t } = useAccessI18n();
  const home = `${PUBLIC_SITE_URL.replace(/\/$/, "")}/${locale}`;
  return (
    <main className="auth-page">
      <aside className="auth-story-panel">
        <AuthPhotoCarousel />
        <a className="auth-story-logo" href={home} aria-label={t("layout.homeAria")}><img src="/onecrew-logo-dark.svg" alt="OneCrew" width="740" height="161" /></a>
      </aside>
      <section className="auth-form-panel">
        <header className="auth-header">
          <a
            href={home}
            aria-label={t("layout.homeAria")}
            className="auth-mobile-brand"
          >
            <img src="/onecrew-logo-color.svg" alt="OneCrew" width="740" height="161" />
          </a>
          <nav aria-label={t("auth.design.pageOptions")}><a className="auth-back-link" href={home}>{t("auth.design.back")}</a><AccessLanguageToggle iconOnly /></nav>
        </header>
        <div className="auth-form-shell">{children}</div>
        <footer className="auth-legal-line">© 2026 OneCrew</footer>
      </section>

    </main>
  );
}
