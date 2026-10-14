import { z } from "zod";
import { getDeviceRevocationTarget } from "./service";
import type { Bindings } from "../types/bindings";
import { IntegrationError } from "./errors";

export type DeviceOwner = { userId: string; deviceId: string; pairingId: string; installationId: string; credentialId: string };

// This projection is owned by Sync's /creator-sync/credential contract. Access
// validates it; it never implements or caches Sync's credential verification.
const credentialProjection = z
  .object({
    protocol: z.literal("reacher.creator_sync"),
    protocol_version: z.literal(1),
    installation_id: z.string().uuid(),
    credential_id: z.string().uuid(),
    access_user_id: z.string().min(1).max(128),
    access_device_id: z.string().regex(/^dev_[A-Za-z0-9_-]{24}$/),
    access_pairing_id: z.string().regex(/^pair_[A-Za-z0-9_-]{24}$/),
    source_instance_id: z.string().uuid(),
    capabilities: z.tuple([
      z.literal("creator_snapshots:write_chunked_quarantine"),
    ]),
    accepted_bundle: z
      .object({
        format: z.literal("reacher.creator_bundle"),
        // Bundle capabilities can evolve without changing the authenticated
        // device identity. Only explicitly supported capability versions pass.
        format_version: z.union([z.literal(1), z.literal(2)]),
        profile: z.literal("portable_public"),
        max_snapshot_bytes: z.number().int().positive(),
        max_chunk_bytes: z.number().int().positive(),
        max_chunks: z.number().int().positive(),
        max_namespace_staged_bytes: z.number().int().positive(),
        max_access_user_staged_bytes: z.number().int().positive(),
      })
      .strict(),
  })
  .strict();

function probeUrl(bindings: Bindings) {
  const configured = bindings.CREATOR_SYNC_API_ORIGIN;
  if (!configured || !bindings.SYNC_CREDENTIAL_VERIFIER) {
    throw new IntegrationError("configuration_required", 503);
  }
  try {
    const url = new URL(configured);
    if (
      url.protocol !== "https:" ||
      url.origin !== configured ||
      url.port ||
      url.username ||
      url.password ||
      url.pathname !== "/" ||
      url.search ||
      url.hash
    )
      throw new Error("invalid_origin");
    return new URL("/creator-sync/credential", url).href;
  } catch {
    throw new IntegrationError("configuration_required", 503);
  }
}

async function limitedJson(response: Response) {
  if (
    response.headers.get("Content-Type")?.split(";", 1)[0]?.trim() !==
      "application/json" ||
    !response.body
  ) {
    throw new Error("invalid_identity_response");
  }
  const reader = response.body.getReader();
  const parts: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > 16_384) throw new Error("identity_response_too_large");
      parts.push(value);
    }
  } finally {
    await reader.cancel().catch(() => undefined);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  return JSON.parse(new TextDecoder().decode(bytes));
}

export async function verifyDeviceCredential(
  bindings: Bindings,
  db: D1Database,
  request: Request,
): Promise<DeviceOwner> {
  const authorization = request.headers.get("Authorization");
  const match =
    authorization &&
    /^Bearer ([0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})_([A-Za-z0-9_-]{43,128})$/i.exec(
      authorization,
    );
  if (!match) throw new IntegrationError("device_unauthorized", 401);
  const url = probeUrl(bindings);
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      controller.abort();
      reject(new IntegrationError("identity_service_unavailable", 503, true));
    }, 5000);
  });
  let identity: z.infer<typeof credentialProjection>;
  try {
    identity = await Promise.race([
      (async () => {
        const response = await bindings.SYNC_CREDENTIAL_VERIFIER!.fetch(
          new Request(url, {
            method: "GET",
            // Never forward Cookie, caller URLs or unrelated headers to Sync.
            headers: {
              Authorization: authorization,
              Accept: "application/json",
            },
            redirect: "manual",
            signal: AbortSignal.any([request.signal, controller.signal]),
          }),
        );
        if (response.status === 401 || response.status === 403) {
          await response.body?.cancel();
          throw new IntegrationError("device_unauthorized", 401);
        }
        if (!response.ok || response.redirected) {
          await response.body?.cancel();
          throw new IntegrationError("identity_service_unavailable", 503, true);
        }
        return credentialProjection.parse(await limitedJson(response));
      })(),
      deadline,
    ]);
  } catch (error) {
    if (error instanceof IntegrationError) throw error;
    throw new IntegrationError("identity_service_unavailable", 503, true);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
  if (identity.credential_id !== match[1]) {
    throw new IntegrationError("identity_service_unavailable", 503);
  }
  let target;
  try {
    target = await getDeviceRevocationTarget(
      db,
      identity.access_user_id,
      identity.access_device_id,
    );
  } catch {
    throw new IntegrationError("storage_unavailable", 503, true);
  }
  if (
    !target ||
    !target.consumed ||
    target.status !== "active" ||
    target.pairingId !== identity.access_pairing_id
  ) {
    throw new IntegrationError("device_inactive", 403);
  }
  return {
    userId: identity.access_user_id,
    deviceId: identity.access_device_id,
    pairingId: identity.access_pairing_id,
    installationId: identity.installation_id,
    credentialId: identity.credential_id,
  };
}
