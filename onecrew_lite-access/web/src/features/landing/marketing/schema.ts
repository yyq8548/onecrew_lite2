import { PUBLIC_CONTACT } from "@/lib/public-contact";
import { localizedPath } from "@/lib/site-locale";

import { absoluteUrl } from "./site";
import type { MarketingLocale } from "./use-marketing-content";

export type BreadcrumbSchemaItem = {
  href?: string;
  label: string;
};

export const oneCrewOrganization = {
  "@type": "Organization",
  name: "OneCrew",
  email: PUBLIC_CONTACT.email,
  sameAs: [PUBLIC_CONTACT.x],
  url: absoluteUrl("/"),
  logo: absoluteUrl("/onecrew-ai-mark.png"),
} as const;

export function localizedAbsoluteUrl(path: string, locale: MarketingLocale) {
  return absoluteUrl(localizedPath(path, locale));
}

export function breadcrumbList(items: BreadcrumbSchemaItem[], locale: MarketingLocale) {
  return {
    "@type": "BreadcrumbList",
    itemListElement: items.map((item, index) => ({
      "@type": "ListItem",
      position: index + 1,
      name: item.label,
      ...(item.href ? { item: localizedAbsoluteUrl(item.href, locale) } : {}),
    })),
  };
}

export function schemaGraph(...items: unknown[]) {
  return {
    "@context": "https://schema.org",
    "@graph": items,
  };
}
