import { useLocation } from "react-router";

import { AccessRoot } from "@/access-app";
import { SiteApp } from "@/site-app";

export { AppRoutes } from "@/access-app";

const ACCESS_APP_PATHS = new Set([
  "/login",
  "/register",
  "/forgot-password",
  "/account",
  "/pairing/authorize",
  "/pairing/rate-limited",
]);

export function App() {
  const location = useLocation();
  const normalizedPath = location.pathname.length > 1
    ? location.pathname.replace(/\/+$/, "")
    : location.pathname;

  return (ACCESS_APP_PATHS.has(normalizedPath) || normalizedPath.startsWith("/d/")) ? <AccessRoot /> : <SiteApp />;
}
