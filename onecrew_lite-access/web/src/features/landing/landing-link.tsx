import type { ComponentProps } from "react";
import { Link as RouterLink } from "react-router";

import { localizedPath } from "@/lib/site-locale";

import { useLandingLocale } from "./language-switcher";

type LandingLinkProps = Omit<ComponentProps<typeof RouterLink>, "to"> & {
  href: string;
};

export function LandingLink({ href, ...props }: LandingLinkProps) {
  const locale = useLandingLocale();
  return <RouterLink to={localizedPath(href, locale)} {...props} />;
}
