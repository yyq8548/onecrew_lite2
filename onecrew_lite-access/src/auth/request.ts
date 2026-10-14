import type { Context } from "hono";
import type { AppEnv } from "../types/bindings";

const PRIVATE_AUTH_REQUEST_HEADERS = [
  "Origin",
  "Cookie",
  "User-Agent",
  "cf-connecting-ip",
  "Accept-Language",
] as const;

export function privateAuthHeaders(source: Headers, contentType?: string) {
  const headers = new Headers();
  for (const name of PRIVATE_AUTH_REQUEST_HEADERS) {
    const value = source.get(name);
    if (value) headers.set(name, value);
  }
  if (contentType) headers.set("Content-Type", contentType);
  return headers;
}

export function callPrivateAuthHandler(
  context: Context<AppEnv>,
  input: {
    body?: string;
    method: string;
    path: string;
    query?: URLSearchParams;
  },
) {
  const url = new URL(input.path, context.get("config").authOrigin);
  if (input.query) url.search = input.query.toString();
  const request = new Request(url, {
    ...(input.body === undefined ? {} : { body: input.body }),
    headers: privateAuthHeaders(
      context.req.raw.headers,
      input.body === undefined ? undefined : "application/json",
    ),
    method: input.method,
    redirect: "manual",
  });
  return context.get("auth").handler(request);
}

export function hasJsonContentType(headers: Headers) {
  const contentType = headers.get("Content-Type");
  return contentType?.split(";", 1)[0]?.trim().toLowerCase() === "application/json";
}
