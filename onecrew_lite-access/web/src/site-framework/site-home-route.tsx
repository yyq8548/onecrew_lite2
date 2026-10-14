import { HomeRoute } from "@/features/landing/marketing/marketing-routes";
import { RouteScrollTop } from "@/features/landing/marketing/route-scroll-top";
import { DocumentLanguage } from "@/lib/document-language";

export default function SiteHomeRoute() {
  return (
    <>
      <DocumentLanguage />
      <RouteScrollTop />
      <HomeRoute />
    </>
  );
}
