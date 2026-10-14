import { MarketingRoutes } from "@/features/landing/marketing/marketing-routes";
import { RouteScrollTop } from "@/features/landing/marketing/route-scroll-top";
import { DocumentLanguage } from "@/lib/document-language";
import { stripLocalePrefix } from "@/lib/site-locale";
import { useLocation } from "react-router";

export function SiteApp() {
  const location = useLocation();
  return (
    <>
      <DocumentLanguage />
      <RouteScrollTop />
      <MarketingRoutes location={{ ...location, pathname: stripLocalePrefix(location.pathname) }} />
    </>
  );
}
