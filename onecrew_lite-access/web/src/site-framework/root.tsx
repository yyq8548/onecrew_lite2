import { Links, Meta, Outlet, Scripts, useLocation } from "react-router";

import { localeFromPathname } from "@/lib/site-locale";
import { SiteI18nProvider } from "@/lib/site-i18n";
import { AnalyticsNavigation } from "@/lib/analytics-navigation";
import "@/site-styles.css";

export function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const locale = localeFromPathname(location.pathname);

  return (
    <html lang={locale === "en" ? "en" : "zh-CN"}>
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <link rel="icon" href="/onecrew-ai-mark.png" type="image/png" sizes="512x512" />
        <link rel="apple-touch-icon" href="/onecrew-ai-mark.png" />
        <Meta />
        <Links />
        <script src="/ga4-init.js" data-surface="marketing" />
        <script async src="https://www.googletagmanager.com/gtag/js?id=G-E8PCYC26GN" />
      </head>
      <body>
        <AnalyticsNavigation />
        {children}
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return (
    <SiteI18nProvider>
      <Outlet />
    </SiteI18nProvider>
  );
}
