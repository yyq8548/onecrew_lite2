import { invoke } from "@/lib/local-runtime";
import { listen, type UnlistenFn } from "@/lib/runtime-events";

import type { LoginProjection } from "@/lib/generated/creator-sync-contracts";
import { validateLoginProjection } from "@/lib/generated/creator-sync-contracts/validators/LoginProjection.js";

export const CREATOR_SYNC_PAIRING_STATUS_CHANGED_EVENT =
  "creator-sync://pairing-status-changed";

let pendingAuthorization: { flowId: string; url: string } | null = null;

export function pairingAuthorizationUrl(flowId: string | null | undefined): string | undefined {
  return pendingAuthorization && pendingAuthorization.flowId === flowId ? pendingAuthorization.url : undefined;
}

export function readCreatorSyncPairingStatus(): Promise<LoginProjection> {
  return invokeProjection("creator_sync_pairing_status");
}

export function beginCreatorSyncPairing(): Promise<LoginProjection> {
  return authorizeWithBrowser("creator_sync_pairing_begin");
}

export function reopenCreatorSyncPairing(): Promise<LoginProjection> {
  return authorizeWithBrowser("creator_sync_pairing_reopen");
}

export function cancelCreatorSyncPairing(): Promise<LoginProjection> {
  return invokeProjection("creator_sync_pairing_cancel");
}

export function subscribeCreatorSyncPairingStatus(
  onProjection: (projection: LoginProjection) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CREATOR_SYNC_PAIRING_STATUS_CHANGED_EVENT, ({ payload }) => {
    // The event is advisory and polling remains available, so malformed native
    // data is ignored without forwarding or logging the untrusted payload.
    if (!validateLoginProjection(payload)) return;
    acceptProjection(payload);
    onProjection(payload);
  });
}

async function invokeProjection(command: string): Promise<LoginProjection> {
  const result = await invoke<unknown>(command);
  if (!validateLoginProjection(result)) {
    throw new Error("Creator Sync pairing returned an invalid projection");
  }
  acceptProjection(result);
  return result;
}

function acceptProjection(projection: LoginProjection) {
  if (projection.status.state !== "pairing_pending" || projection.flow_id !== pendingAuthorization?.flowId) {
    pendingAuthorization = null;
  }
}

async function authorizeWithBrowser(command: string): Promise<LoginProjection> {
  // Reserve the tab during the user's gesture, before awaiting the local
  // command. A blocked popup leaves a normal link on the waiting screen.
  const popup = window.open("about:blank", "_blank");
  if (popup) popup.opener = null;
  try {
    const result = await invoke<unknown>(command);
    if (typeof result !== "object" || result === null || !("authorize_url" in result) || typeof result.authorize_url !== "string" || !("projection" in result) || !validateLoginProjection(result.projection)) {
      throw new Error("Creator Sync pairing returned an invalid authorization response");
    }
    const url = new URL(result.authorize_url);
    if (url.protocol !== "https:" || url.username || url.password || !result.projection.flow_id) {
      throw new Error("Creator Sync pairing returned an invalid authorization URL");
    }
    pendingAuthorization = { flowId: result.projection.flow_id, url: url.href };
    if (popup) popup.location.replace(url.href);
    return result.projection;
  } catch (error) {
    popup?.close();
    throw error;
  }
}
