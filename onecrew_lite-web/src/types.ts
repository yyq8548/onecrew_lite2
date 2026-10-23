// 网页共享账号类型；业务数据通过当前 TypeScript 服务的 HTTP 接口读取。

/** A platform OneCrew can hold a connected operator account for. This is also
 * the set with a direct-message inbox — see `@/lib/dm-platforms`, which lists
 * it at runtime. Display names come from `PLATFORM_LABEL` in `@/lib/creator`,
 * which covers every platform a Creator can exist on. */
export type Platform = "x" | "instagram" | "telegram";

export type XChatPasscodeStatus =
  | "configured"
  | "not_configured"
  | "unavailable"
  | "not_applicable";

/** One connected account, mirroring the backend `ProfileRow`. */
export type Account = {
  id: string;
  platform: Platform;
  /** Stable provider-side account identifier. */
  provider_account_id: string;
  /** Signed-in @handle. */
  username: string;
  /** User-given Chrome profile name (empty for Telegram). */
  profile_name: string;
  /** Fingerprint summary for display, e.g. `Chrome · en-US · America/Los_Angeles` (empty for Telegram). */
  fingerprint_seed: string;
  /** Configured proxy gateway `host:port`, or null for a direct connection. */
  proxy?: string | null;
  /** Proxy protocol: `http` / `https` / `socks5` (null when direct). */
  proxy_kind?: string | null;
  /** Real detected egress (`ip · country`), shown in preference to the gateway. */
  proxy_egress?: string | null;
  /** Secret-free X Chat passcode state; the passcode itself is never returned. */
  x_chat_passcode_status: XChatPasscodeStatus;
  /** Epoch seconds (as a string) of the last update. */
  updated_at: string;
};

/** Safe service-credential metadata; the encrypted secret is never returned. */
export type ServiceCredential = {
  id: string;
  provider: "youtube" | "resend";
  label: string;
  metadata: Record<string, unknown>;
  disabled: boolean;
  cooldown_until?: number | null;
  last_used_at?: number | null;
  last_error?: string | null;
  created_at: string;
  updated_at: string;
};

/** Secret-free status projection returned by service-credential mutations. */
export type ServiceCredentialProjection = {
  account_ref_id: string;
  provider: string;
  label: string;
  active: boolean;
  available: boolean;
  disabled: boolean;
  cooldown_until: number | null;
  last_error: string | null;
};

export type YoutubeCredentialTest = {
  credential_id: string;
  ok: boolean;
  checked_at: number;
  channel_id?: string | null;
  issue?: "api_key_invalid" | "access_not_configured" | null;
  message: string;
};

export type ResendCredentialTest = {
  credential_id: string;
  ok: boolean;
  checked_at: number;
  email_address: string;
  message: string;
};

/** Result of the validate-first service credential connection use case. */
export type ServiceCredentialConnectResult = {
  credential: {
    credential_id: string;
    provider: "youtube" | "resend";
    label: string;
    /** Typed provider details; exactly one member is present per provider. */
    youtube?: { project?: string | null } | null;
    mailbox?: { email_address: string; display_name?: string | null } | null;
    available: boolean;
  };
  health: {
    checked_at: number;
    message: string;
    provider_resource_id?: string | null;
  };
};

/** Transient YouTube credential inputs accepted by the Tauri command. */
export type YoutubeApiKeyConnectInput = {
  label: string;
  project: string | null;
  api_key: string;
};

/** Replace one existing YouTube key without changing its label or project. */
export type RotateYoutubeCredentialInput = {
  credential_id: string;
  api_key: string;
  confirmed: true;
};

/** Rename one existing YouTube credential without supplying its secret. */
export type RenameYoutubeCredentialInput = {
  credential_id: string;
  label: string;
  confirmed: true;
};

export type YoutubeCredentialDeleteInput = {
  credentialId: string;
  confirmed: true;
  idempotencyKey: string;
};

export type YoutubeCredentialDeleteOutcome = {
  credential_id: string;
  replayed: boolean;
};
