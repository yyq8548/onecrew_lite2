function requiredHttpsOrigin(value: string | undefined, name: string) {
  if (!value) throw new Error(`${name} must be configured at build time`);
  const url = new URL(value);
  if (
    url.protocol !== "https:"
    || url.origin !== value
    || url.pathname !== "/"
    || url.search !== ""
    || url.hash !== ""
    || url.username !== ""
    || url.password !== ""
  ) {
    throw new Error(`${name} must be an HTTPS origin without a path`);
  }
  return url.origin;
}

export const PUBLIC_SITE_URL = requiredHttpsOrigin(
  import.meta.env.VITE_SITE_URL,
  "VITE_SITE_URL",
);

export const PUBLIC_BLOG_URL = requiredHttpsOrigin(
  import.meta.env.VITE_BLOG_URL,
  "VITE_BLOG_URL",
);
