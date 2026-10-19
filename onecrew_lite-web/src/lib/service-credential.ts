import type { ServiceCredential } from "@/types";

export function serviceCredentialIsAvailable(
  credential: ServiceCredential,
  now = Date.now(),
): boolean {
  return !credential.disabled
    && !credential.last_error
    && !(credential.cooldown_until && credential.cooldown_until * 1000 > now);
}
