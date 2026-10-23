type ClientError = { command: string; code: string; at: number };
let owner: string | null = null;
let errors: ClientError[] = [];
let failedDraft: Record<string, unknown> | null = null;
export function setRepairOwner(selected: string | null): void {
  if (selected !== owner) { owner = selected; errors = []; failedDraft = null; }
}
export function recordRepairError(command: string, error: unknown): void {
  if (!owner || command.startsWith("onecrew_repair_")) return;
  const value = error && typeof error === "object" && "code" in error ? error.code : error instanceof Error ? error.message : error;
  const code = typeof value === "string" && /^[a-zA-Z][a-zA-Z0-9_.:-]{0,95}$/u.test(value) ? value : "client.request_failed";
  errors = [...errors, { command, code, at: Date.now() }].slice(-8);
}
export function recordFailedRepairDraft(input: Record<string, unknown> | null): void { if (owner) failedDraft = input === null ? null : structuredClone(input); }
export async function repairClientContext() {
  const campaign = await Promise.resolve().then(() => window.localStorage.getItem("onecrew-opencli-pilot-campaign"))
    .then(value => value, () => { recordRepairError("repair_context", "client.storage_unavailable"); return null; });
  const campaign_id = campaign && /^[A-Za-z\d_-]{1,128}$/u.test(campaign) ? campaign : null;
  return { client_errors: [...errors], failed_draft: failedDraft, campaign_id };
}
