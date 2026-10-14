import { z } from "zod";

import { accessApiURL } from "@/lib/access-origin";
import { currentAccessLanguage } from "@/lib/request-locale";

const safeUserSchema = z
  .object({
    email: z.email(),
    id: z.string().min(1),
    image: z.string().nullable(),
    name: z.string(),
  })
  .strict();

const anonymousSessionSchema = z
  .object({
    authenticated: z.literal(false),
    expiresAt: z.null(),
    user: z.null(),
  })
  .strict();

const authenticatedSessionSchema = z
  .object({
    authenticated: z.literal(true),
    expiresAt: z.iso.datetime({ offset: true }),
    user: safeUserSchema,
  })
  .strict();

const safeSessionSchema = z.discriminatedUnion("authenticated", [
  anonymousSessionSchema,
  authenticatedSessionSchema,
]);

const authCapabilitiesSchema = z
  .object({
    emailOtpEnabled: z.boolean().optional(),
    googleSignInEnabled: z.boolean(),
  })
  .strict();

const projectedUserSchema = safeUserSchema;
const authSuccessSchema = z.object({
  ok: z.literal(true),
  user: projectedUserSchema,
}).strict();
const loginChallengeSchema = z.object({
  ok: z.literal(false),
  challenge: z.literal("email_otp"),
  email: z.email(),
  reason: z.enum(["new_ip", "email_verification"]),
}).strict();
const loginResponseSchema = z.union([authSuccessSchema, loginChallengeSchema]);
const registrationResponseSchema = z.object({
  ok: z.literal(true),
  verification_required: z.boolean().optional(),
  email: z.email().optional(),
  user: projectedUserSchema.optional(),
}).strict();
const simpleSuccessSchema = z.object({ ok: z.literal(true) }).strict();
const accountSettingsSchema = z.object({
  hasPassword: z.boolean(),
  user: projectedUserSchema,
}).strict();
const accountMutationSchema = z.union([authSuccessSchema, simpleSuccessSchema]);

const deviceSummarySchema = z
  .object({
    appVersion: z.string().min(1).max(32),
    createdAt: z.iso.datetime({ offset: true }),
    deviceId: z.string().min(1).max(128),
    label: z.string().min(1).max(80),
    pairedAt: z.iso.datetime({ offset: true }).nullable(),
    pairingExpiresAt: z.iso.datetime({ offset: true }),
    pairingId: z.string().min(1).max(128),
    pairingIp: z.string().regex(/^[0-9A-Fa-f:.]{3,64}$/).nullable(),
    pairingLocation: z.object({
      country: z.string().max(8).nullable(),
      region: z.string().max(128).nullable(),
      city: z.string().max(128).nullable(),
      colo: z.string().max(8).nullable(),
    }).strict(),
    platform: z.enum(["linux", "macos", "windows"]),
    revokedAt: z.iso.datetime({ offset: true }).nullable(),
    status: z.enum(["active", "expired", "pending", "revoked"]),
  })
  .strict();

const devicePageSchema = z
  .object({
    devices: z.array(deviceSummarySchema).max(20),
    nextCursor: z.string().min(1).max(128).nullable(),
  })
  .strict();

const deviceEnvelopeSchema = z.object({ device: deviceSummarySchema }).strict();
const deletedDeviceEnvelopeSchema = z
  .object({ deletedDeviceId: z.string().min(1).max(128) })
  .strict();

const pairingPreviewSchema = z
  .object({
    approvalToken: z.string().min(160).max(1_500),
    expiresAt: z.iso.datetime({ offset: true }),
    request: z
      .object({
        appName: z.literal("Agent Platform"),
        appVersion: z.string().min(1).max(32),
        callbackPort: z
          .string()
          .regex(/^[1-9][0-9]{3,4}$/)
          .refine((value) => Number(value) >= 1024 && Number(value) <= 65_535),
        label: z.string().min(1).max(80),
        platform: z.enum(["linux", "macos", "windows"]),
        platformLabel: z.enum(["Linux", "macOS", "Windows"]),
        bindingIp: z.string().regex(/^[0-9A-Fa-f:.]{3,64}$/).nullable(),
        bindingLocation: z.object({
          country: z.string().max(8).nullable(),
          region: z.string().max(128).nullable(),
          city: z.string().max(128).nullable(),
          colo: z.string().max(8).nullable(),
        }).strict(),
      })
      .strict(),
  })
  .strict();

const publicErrorCodeSchema = z.enum([
  "account_settings_failed",
  "authentication_required",
  "device_limit_reached",
  "device_must_be_revoked",
  "device_revocation_conflict",
  "device_revocation_unavailable",
  "device_not_found",
  "email_otp_invalid",
  "email_otp_unavailable",
  "email_change_failed",
  "internal_server_error",
  "invalid_device_cursor",
  "invalid_device_request",
  "invalid_content_type",
  "invalid_pairing_request",
  "login_failed",
  "logout_failed",
  "oauth_provider_not_found",
  "origin_not_allowed",
  "pairing_approval_expired",
  "pairing_approval_resolved",
  "password_update_failed",
  "profile_update_failed",
  "password_reset_failed",
  "rate_limited",
  "registration_failed",
  "request_too_large",
  "service_configuration_error",
]);

const safeErrorEnvelopeSchema = z
  .object({ error: publicErrorCodeSchema })
  .strict();

const ignoredResponseSchema = z.unknown();

export type SafeSession = z.infer<typeof safeSessionSchema>;
export type SafeSessionUser = z.infer<typeof safeUserSchema>;
export type AuthCapabilities = z.infer<typeof authCapabilitiesSchema>;
export type AccountSettings = z.infer<typeof accountSettingsSchema>;
export type AccessErrorCode = z.infer<typeof publicErrorCodeSchema>;
export type DeviceSummary = z.infer<typeof deviceSummarySchema>;
export type DevicePage = z.infer<typeof devicePageSchema>;
export type PairingPreview = z.infer<typeof pairingPreviewSchema>;

export function isSafeSessionAuthenticated(
  session: SafeSession,
  now = Date.now(),
): session is Extract<SafeSession, { authenticated: true }> {
  return session.authenticated && Date.parse(session.expiresAt) > now;
}

export class AccessApiError extends Error {
  readonly code?: AccessErrorCode;
  readonly status: number;

  constructor(status: number, code?: AccessErrorCode) {
    super("Access API request failed");
    this.name = "AccessApiError";
    this.status = status;
    this.code = code;
  }
}

export class AccessContractError extends Error {
  constructor() {
    super("Access API response violated its public contract");
    this.name = "AccessContractError";
  }
}

export async function getSafeSession() {
  return requestJSON("/api/session", { method: "GET" }, safeSessionSchema);
}

export async function getAuthCapabilities() {
  return requestJSON(
    "/api/auth/capabilities",
    { method: "GET" },
    authCapabilitiesSchema,
  );
}

export async function registerAccessSession(input: {
  email: string;
  name: string;
  password: string;
}) {
  return requestJSON(
    "/api/session/register",
    {
      body: JSON.stringify(input),
      method: "POST",
    },
    registrationResponseSchema,
  );
}

export async function loginAccessSession(input: {
  email: string;
  password: string;
}) {
  return requestJSON(
    "/api/session/login",
    {
      body: JSON.stringify(input),
      method: "POST",
    },
    loginResponseSchema,
  );
}

export async function sendLoginOTP(email: string) {
  return requestJSON(
    "/api/session/login-otp/send",
    {
      body: JSON.stringify({ email }),
      method: "POST",
    },
    simpleSuccessSchema,
  );
}

export async function sendRegistrationOTP(email: string) {
  return requestJSON(
    "/api/session/register/verify/send",
    {
      body: JSON.stringify({ email }),
      method: "POST",
    },
    simpleSuccessSchema,
  );
}

export async function loginWithOTP(input: { email: string; otp: string }) {
  return requestJSON(
    "/api/session/login-otp",
    {
      body: JSON.stringify(input),
      method: "POST",
    },
    authSuccessSchema,
  );
}

export async function verifyRegistrationOTP(input: { email: string; otp: string }) {
  return requestJSON(
    "/api/session/register/verify",
    {
      body: JSON.stringify(input),
      method: "POST",
    },
    authSuccessSchema,
  );
}

export async function getAccountSettings() {
  return requestJSON("/api/account/settings", { method: "GET" }, accountSettingsSchema);
}

export async function updateAccountProfile(input: { name: string }) {
  return requestJSON(
    "/api/account/profile",
    { body: JSON.stringify(input), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function updateAccountPassword(input: {
  currentPassword?: string;
  newPassword: string;
}) {
  return requestJSON(
    "/api/account/password",
    { body: JSON.stringify(input), method: "POST" },
    accountMutationSchema,
  );
}

export async function sendCurrentEmailOTP() {
  return requestJSON(
    "/api/account/email/current-otp/send",
    { body: "{}", method: "POST" },
    simpleSuccessSchema,
  );
}

export async function verifyCurrentEmailOTP(otp: string) {
  return requestJSON(
    "/api/account/email/current-otp/verify",
    { body: JSON.stringify({ otp }), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function requestEmailChange(input: { newEmail: string; currentOtp: string }) {
  return requestJSON(
    "/api/account/email/change/request",
    { body: JSON.stringify(input), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function verifyEmailChange(input: { newEmail: string; otp: string }) {
  return requestJSON(
    "/api/account/email/change/verify",
    { body: JSON.stringify(input), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function requestPasswordResetOTP(email: string) {
  return requestJSON(
    "/api/session/forgot-password/send",
    { body: JSON.stringify({ email }), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function resetPasswordWithOTP(input: { email: string; otp: string; password: string }) {
  return requestJSON(
    "/api/session/forgot-password",
    { body: JSON.stringify(input), method: "POST" },
    simpleSuccessSchema,
  );
}

export async function logoutAccessSession() {
  await requestJSON(
    "/api/session/logout",
    {
      body: "{}",
      method: "POST",
    },
    ignoredResponseSchema,
  );
}

export async function getDesktopPairingPreview(
  search: string,
  options: { signal?: AbortSignal } = {},
) {
  if (!search.startsWith("?") || search.includes("#")) {
    throw new AccessContractError();
  }
  return requestJSON(
    `/api/pairing/authorize${search}`,
    {
      method: "GET",
      ...(options.signal ? { signal: options.signal } : {}),
    },
    pairingPreviewSchema,
  );
}

export async function listAccessDevices(
  cursor: string | null = null,
  options: { signal?: AbortSignal } = {},
) {
  const path = cursor
    ? (`/api/devices?${new URLSearchParams({ cursor }).toString()}` as const)
    : "/api/devices";
  return requestJSON(
    path,
    {
      method: "GET",
      ...(options.signal ? { signal: options.signal } : {}),
    },
    devicePageSchema,
  );
}

export async function getAccessDeviceStatus(
  deviceId: string,
  options: { signal?: AbortSignal } = {},
) {
  const envelope = await requestJSON(
    `/api/devices/${encodeURIComponent(deviceId)}/status`,
    {
      method: "GET",
      ...(options.signal ? { signal: options.signal } : {}),
    },
    deviceEnvelopeSchema,
  );
  return envelope.device;
}

export async function revokeAccessDevice(
  deviceId: string,
  options: { signal?: AbortSignal } = {},
) {
  const envelope = await requestJSON(
    `/api/devices/${encodeURIComponent(deviceId)}/revoke`,
    {
      body: "{}",
      method: "POST",
      ...(options.signal ? { signal: options.signal } : {}),
    },
    deviceEnvelopeSchema,
  );
  return envelope.device;
}

export async function deleteAccessDevice(
  deviceId: string,
  options: { signal?: AbortSignal } = {},
) {
  const envelope = await requestJSON(
    `/api/devices/${encodeURIComponent(deviceId)}`,
    {
      method: "DELETE",
      ...(options.signal ? { signal: options.signal } : {}),
    },
    deletedDeviceEnvelopeSchema,
  );
  return envelope.deletedDeviceId;
}

async function requestJSON<T>(
  path: `/api/${string}`,
  init: RequestInit,
  schema: z.ZodType<T>,
) {
  const hasBody = init.body !== undefined;
  const response = await fetch(accessApiURL(path), {
    ...init,
    cache: "no-store",
    credentials: "include",
    headers: {
      Accept: "application/json",
      "Accept-Language": currentAccessLanguage(),
      ...(hasBody ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
  });

  if (!response.ok) {
    throw new AccessApiError(response.status, await readErrorCode(response));
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new AccessContractError();
  }

  const parsed = schema.safeParse(body);
  if (!parsed.success) {
    throw new AccessContractError();
  }
  return parsed.data;
}

async function readErrorCode(response: Response) {
  try {
    const body: unknown = await response.json();
    const parsed = safeErrorEnvelopeSchema.safeParse(body);
    return parsed.success ? parsed.data.error : undefined;
  } catch {
    // The public UI deliberately ignores unstructured backend error bodies.
  }

  return undefined;
}
