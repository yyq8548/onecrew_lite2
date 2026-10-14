export type EdgeRateLimitGroup =
  | "cli_install"
  | "integration"
  | "auth"
  | "device"
  | "pairing_read"
  | "pairing_write"
  | "session";

const IP_ADDRESS_PATTERN = /^[0-9A-Fa-f:.]{3,64}$/;

export function edgeRateLimitGroup(
  method: string,
  pathname: string,
): EdgeRateLimitGroup | null {
  if (pathname === "/api/cli/install-links" || pathname.startsWith("/cli/install/") || pathname.startsWith("/cli/download/")) return "cli_install";
  if (
    pathname.startsWith("/api/desktop/integrations/mail/v1/") ||
    pathname === "/integrations/mail/connect" ||
    pathname === "/integrations/gmail/callback"
  )
    return "integration";
  if (
    pathname === "/api/session/register" ||
    pathname === "/api/session/login" ||
    pathname.startsWith("/api/session/forgot-password")
  ) {
    return "auth";
  }
  if (
    (method === "POST" && pathname === "/api/auth/sign-in/social") ||
    (method === "GET" && pathname === "/api/auth/callback/google")
  ) {
    return "auth";
  }
  if (pathname === "/api/session" || pathname === "/api/session/logout") {
    return "session";
  }
  if (pathname.startsWith("/api/account/")) {
    return "session";
  }
  if (pathname === "/api/devices" || pathname.startsWith("/api/devices/")) {
    return "device";
  }
  if (method === "GET" && pathname === "/api/pairing/authorize") {
    return "pairing_read";
  }
  if (method === "POST" && pathname === "/api/pairing/authorize") {
    return "pairing_write";
  }
  return null;
}

export function edgeRateLimitKey(
  group: EdgeRateLimitGroup,
  connectingIp: string | undefined,
) {
  const client =
    connectingIp !== undefined && IP_ADDRESS_PATTERN.test(connectingIp)
      ? connectingIp
      : "unknown-client";
  return `${group}:${client}`;
}

export async function passesEdgeRateLimit(
  limiter: RateLimit,
  group: EdgeRateLimitGroup,
  connectingIp: string | undefined,
) {
  try {
    return (await limiter.limit({ key: edgeRateLimitKey(group, connectingIp) }))
      .success;
  } catch {
    // This permissive in-Worker gate is not an authorization invariant. Our
    // wrapper allows on binding errors; Better Auth and Access D1 remain exact.
    return true;
  }
}
