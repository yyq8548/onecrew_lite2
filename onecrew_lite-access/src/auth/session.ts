const FORWARDED_AUTH_HEADERS = new Set([
  "cache-control",
  "pragma",
  "set-cookie",
]);

type HeadersWithSetCookie = Headers & {
  getSetCookie?: () => string[];
};

export function authResponseHeaders(sourceHeaders?: HeadersInit) {
  const headers = new Headers({
    "Cache-Control": "no-store",
    Pragma: "no-cache",
    "Referrer-Policy": "no-referrer",
  });

  if (!sourceHeaders) return headers;
  const source =
    sourceHeaders instanceof Headers ? sourceHeaders : new Headers(sourceHeaders);
  for (const [name, value] of source) {
    const lowerName = name.toLowerCase();
    if (FORWARDED_AUTH_HEADERS.has(lowerName) && lowerName !== "set-cookie") {
      headers.set(name, value);
    }
  }

  const sourceWithCookies = source as HeadersWithSetCookie;
  const setCookies = sourceWithCookies.getSetCookie?.();
  if (setCookies && setCookies.length > 0) {
    for (const cookie of setCookies) headers.append("Set-Cookie", cookie);
  } else {
    const setCookie = source.get("Set-Cookie");
    if (setCookie) headers.append("Set-Cookie", setCookie);
  }
  return headers;
}

export function authJsonResponse(
  body: unknown,
  status: number,
  sourceHeaders?: HeadersInit,
) {
  const headers = authResponseHeaders(sourceHeaders);
  headers.set("Content-Type", "application/json; charset=UTF-8");
  return new Response(JSON.stringify(body), { headers, status });
}

export function safeUser(user: {
  email: string;
  id: string;
  image?: string | null | undefined;
  name: string;
}) {
  return {
    email: user.email,
    id: user.id,
    image: user.image ?? null,
    name: user.name,
  };
}

export async function loadSafeSession(
  auth: ReacherAuth,
  requestHeaders: Headers,
) {
  const result = await auth.api.getSession({
    headers: privateAuthHeaders(requestHeaders),
    returnHeaders: true,
  });
  const authSession = result.response;
  return {
    headers: result.headers,
    session: authSession
      ? {
          expiresAt: authSession.session.expiresAt.toISOString(),
          sessionId: authSession.session.id,
          user: safeUser(authSession.user),
        }
      : null,
  };
}
import type { ReacherAuth } from "./auth";
import { privateAuthHeaders } from "./request";
