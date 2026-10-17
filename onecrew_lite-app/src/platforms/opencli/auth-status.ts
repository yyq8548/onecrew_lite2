import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { creatorSearchSource } from "./sources.ts";

export type OpenCliAuthState = "logged_in" | "session_detected" | "not_logged_in" | "unknown" | "error" | "public_read";
export interface OpenCliAuthObservation { platform: string; profile_ref: string; status: OpenCliAuthState; checked_at: number; method: "full" | "quick" | "skipped"; issue: string | null }
export function creatorAuthProbe(platform: string) {
  const source = creatorSearchSource(platform);
  const native = ["bilibili", "tiktok", "instagram", "facebook", "douyin", "linkedin", "weibo", "tieba", "twitter", "pinterest", "youtube"].includes(platform);
  const site = native ? `onecrew-${platform}-auth` : platform === "reddit" ? "reddit" : source.site;
  return { site, full: native || platform === "reddit", addon: native };
}
function issue(error: string) {
  const message = error.toLowerCase();
  if (["http 429", "status 429", "too many requests", "rate limit", "rate-limit"].some(marker => message.includes(marker))) return "auth_probe_rate_limited";
  if (message.includes("navigation rejected")) return "auth_probe_navigation_failed";
  if (message.includes("timeout") || message.includes("timed out")) return "auth_probe_timeout";
  return "auth_probe_failed";
}
export function decodeCreatorAuthStatus(value: unknown, expectedSite: string) {
  const parsed = z.array(z.object({ site: z.literal(expectedSite), status: z.string(), checked: z.string(), error: z.string().default("") })).length(1).safeParse(value);
  if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
  const row = parsed.data[0]!;
  if (row.status === "logged_in" && row.checked === "quick") return { status: "session_detected", method: "quick", issue: null } as const;
  if (row.status === "logged_in" && row.checked === "full") return { status: "logged_in", method: "full", issue: null } as const;
  if (row.status === "not_logged_in" && (row.checked === "full" || row.checked === "quick")) return { status: "not_logged_in" as const, method: row.checked, issue: null };
  if (row.status === "error" && row.checked === "full" && row.error === "COMMAND_EXEC: Current-account identity could not be verified") return { status: "unknown", method: "full", issue: "auth_signal_unavailable" } as const;
  if (row.status === "error" && (row.checked === "full" || row.checked === "quick")) return { status: "error" as const, method: row.checked, issue: issue(row.error) };
  if (row.status === "unknown" && row.checked === "quick") return { status: "unknown", method: "quick", issue: "auth_signal_unavailable" } as const;
  if (row.status === "unknown" && row.checked === "skipped") return { status: "unknown", method: "skipped", issue: "auth_probe_unavailable" } as const;
  throw new OpenCliError("opencli_invalid_output");
}
