import { z } from "zod";
import {
  PAIRING_APPROVAL_TTL_MS,
  desktopPairingAuthorizeRequestSchema,
  type DesktopPairingAuthorizeRequest,
} from "./contracts";
import {
  base64Url,
  constantTimeEqual,
  decodeBase64Url,
  pairingHmac,
} from "./crypto";

const textDecoder = new TextDecoder();
const textEncoder = new TextEncoder();
const approvalPayloadSchema = z
  .object({
    expiresAt: z.number().int().positive(),
    issuedAt: z.number().int().positive(),
    request: desktopPairingAuthorizeRequestSchema,
    sessionBinding: z.string().length(43).regex(/^[A-Za-z0-9_-]{43}$/),
    userId: z.string().min(1).max(128),
    version: z.literal(1),
  })
  .strict();

export const approvalTokenSchema = z
  .string()
  .min(160)
  .max(1_500)
  .regex(/^reacher_pairing_approval_v1\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]{43}$/);

export async function createApprovalToken(
  secret: string,
  userId: string,
  sessionId: string,
  request: DesktopPairingAuthorizeRequest,
  now = Date.now(),
) {
  const sessionBinding = await pairingHmac(secret, "session", sessionId);
  const payload = approvalPayloadSchema.parse({
    expiresAt: now + PAIRING_APPROVAL_TTL_MS,
    issuedAt: now,
    request,
    sessionBinding,
    userId,
    version: 1,
  });
  const encoded = base64Url(textEncoder.encode(JSON.stringify(payload)));
  const signature = await pairingHmac(secret, "approval", encoded);
  return {
    approvalToken: `reacher_pairing_approval_v1.${encoded}.${signature}`,
    expiresAt: new Date(payload.expiresAt).toISOString(),
  };
}

export async function verifyApprovalToken(
  secret: string,
  token: unknown,
  userId: string,
  sessionId: string,
  now = Date.now(),
) {
  const parsedToken = approvalTokenSchema.safeParse(token);
  if (!parsedToken.success) return { kind: "invalid" } as const;
  const [, encoded, signature] = parsedToken.data.split(".");
  if (!encoded || !signature) return { kind: "invalid" } as const;
  const expectedSignature = await pairingHmac(secret, "approval", encoded);
  if (!constantTimeEqual(signature, expectedSignature)) {
    return { kind: "invalid" } as const;
  }

  let payloadValue: unknown;
  try {
    payloadValue = JSON.parse(textDecoder.decode(decodeBase64Url(encoded)));
  } catch {
    return { kind: "invalid" } as const;
  }
  const payload = approvalPayloadSchema.safeParse(payloadValue);
  if (!payload.success || payload.data.userId !== userId) {
    return { kind: "invalid" } as const;
  }
  if (payload.data.expiresAt <= now) return { kind: "expired" } as const;
  const sessionBinding = await pairingHmac(secret, "session", sessionId);
  if (!constantTimeEqual(payload.data.sessionBinding, sessionBinding)) {
    return { kind: "invalid" } as const;
  }
  return { kind: "valid", payload: payload.data } as const;
}
