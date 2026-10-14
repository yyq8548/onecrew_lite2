import { z } from "zod";
export * from "./desktop-pairing-admission-contract";

export const PAIRING_GRANT_TTL_MS = 90_000;
export const PAIRING_APPROVAL_TTL_MS = 5 * 60_000;

const canonicalBase64Url32Schema = z
  .string()
  .length(43)
  .regex(/^[A-Za-z0-9_-]{43}$/)
  .refine(isCanonicalBase64Url32);

const safeLabelSchema = z
  .string()
  .trim()
  .min(1)
  .max(80)
  .transform((value) => value.normalize("NFC"))
  .refine((value) => !/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(value));

export const desktopPairingAuthorizeRequestSchema = z
  .object({
    appVersion: z
      .string()
      .min(1)
      .max(32)
      .regex(/^[0-9A-Za-z]+(?:[.+-][0-9A-Za-z]+)*$/),
    codeChallenge: canonicalBase64Url32Schema,
    codeChallengeMethod: z.literal("S256"),
    callbackPort: z
      .string()
      .regex(/^[1-9][0-9]{3,4}$/)
      .refine((value) => {
        const port = Number(value);
        return port >= 1024 && port <= 65_535;
      }),
    label: safeLabelSchema,
    platform: z.enum(["macos", "windows", "linux"]),
    state: canonicalBase64Url32Schema,
  })
  .strict();

export type DesktopPairingAuthorizeRequest = z.infer<
  typeof desktopPairingAuthorizeRequestSchema
>;

function isCanonicalBase64Url32(value: string) {
  try {
    const decoded = atob(value.replaceAll("-", "+").replaceAll("_", "/") + "=");
    if (decoded.length !== 32) return false;
    let binary = "";
    for (let index = 0; index < decoded.length; index += 1) {
      binary += decoded[index];
    }
    return btoa(binary)
      .replaceAll("+", "-")
      .replaceAll("/", "_")
      .replace(/=+$/, "") === value;
  } catch {
    return false;
  }
}
