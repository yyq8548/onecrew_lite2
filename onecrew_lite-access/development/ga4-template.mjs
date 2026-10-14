import { ACCESS_ORIGIN, SITE_ORIGIN } from "./site-deployment-config.mjs";

export function renderGa4Init(template) {
  for (const placeholder of ["__ONECREW_SITE_ORIGIN__", "__ONECREW_ACCESS_ORIGIN__"]) {
    if (!template.includes(placeholder)) throw new Error(`Missing GA4 placeholder: ${placeholder}`);
  }
  return template
    .replaceAll('"__ONECREW_SITE_ORIGIN__"', JSON.stringify(SITE_ORIGIN))
    .replaceAll('"__ONECREW_ACCESS_ORIGIN__"', JSON.stringify(ACCESS_ORIGIN));
}
