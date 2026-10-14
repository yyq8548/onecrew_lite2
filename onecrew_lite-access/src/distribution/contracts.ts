import { z } from "zod";
export const tokenPattern = /^[A-Za-z0-9_-]{43}$/;
export const versionPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
export const applicationInput = z
  .object({
    displayName: z.string().trim().min(1).max(100),
    purpose: z.string().trim().min(1).max(2000),
    company: z.string().trim().max(200).optional(),
  })
  .strict();
export const reviewInput = z
  .object({
    applicationId: z.string().uuid(),
    expectedRevision: z.number().int().positive(),
    decision: z.enum(["approved", "rejected", "revoked"]),
    reason: z.string().trim().max(2000).optional(),
    requestId: z.string().uuid(),
    actorId: z.string().min(1).max(200),
  })
  .strict()
  .refine((data) => data.decision !== "rejected" || Boolean(data.reason), {
    path: ["reason"],
    message: "User-visible rejection reason is required",
  });
export const retryInput = z
  .object({
    applicationId: z.string().uuid(),
    expectedRevision: z.number().int().positive(),
    expectedMailEventId: z.string().uuid(),
    requestId: z.string().uuid(),
    actorId: z.string().min(1).max(200),
  })
  .strict();
export const assetInput = z
  .object({
    id: z.string().uuid(),
    key: z.string().max(512),
    filename: z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._-]{0,199}$/),
    platform: z.enum(["darwin", "windows"]),
    arch: z.enum(["aarch64", "x86_64"]),
    kind: z.enum(["installer", "updater"]),
    size: z.number().int().positive().safe(),
    sha256: z.string().regex(/^[a-f0-9]{64}$/),
    signature: z.string().trim().min(1).max(4096).optional(),
  })
  .strict();
export const releaseInput = z
  .object({
    version: z.string().max(100).regex(versionPattern),
    notes: z.string().max(20000),
    pubDate: z.iso.datetime({ offset: true }),
    assets: z.array(assetInput).min(1).max(12),
  })
  .strict()
  .superRefine((r, c) => {
    const keys = new Set<string>(),
      targets = new Set<string>(),
      ids = new Set<string>();
    for (const a of r.assets) {
      const t = `${a.platform}/${a.arch}/${a.kind}`;
      if (
        a.key !== `releases/${r.version}/${a.filename}` ||
        keys.has(a.key) ||
        targets.has(t) ||
        ids.has(a.id) ||
        (a.kind === "updater" && !a.signature) ||
        (a.platform === "windows" && (a.arch !== "x86_64" || a.kind !== "installer"))
      )
        c.addIssue({ code: "custom", message: "invalid_asset" });
      keys.add(a.key);
      targets.add(t);
      ids.add(a.id);
    }
    for (const a of r.assets) {
      if (
        a.platform === "darwin" && !r.assets.some(
          (b) =>
            b.platform === a.platform && b.arch === a.arch && b.kind !== a.kind,
        )
      )
        c.addIssue({ code: "custom", message: "incomplete_target" });
    }
  });
export class DistributionError extends Error {
  constructor(
    readonly code: string,
    readonly status: number = 400,
  ) {
    super(code);
  }
}
export function safeDownloadReturnTo(value: unknown): string | null {
  return typeof value === "string" && /^\/d\/[A-Za-z0-9_-]{43}$/.test(value)
    ? value
    : null;
}
export type ApplicationRow = {
  id: string;
  user_id: string;
  display_name: string;
  purpose: string;
  company: string | null;
  locale: "zh-CN" | "en-US";
  review_reason: string | null;
  decision_revision: number | null;
  status: "pending" | "approved" | "rejected";
  revision: number;
  created_at: number;
  updated_at: number;
  last_request_id: string;
};

export type DistributionMailErrorCode =
  | "http_client_error"
  | "rate_limited"
  | "http_server_error"
  | "unexpected_http_status"
  | "timeout"
  | "transport_error"
  | "invalid_provider_response"
  | "persistence_after_ack";
