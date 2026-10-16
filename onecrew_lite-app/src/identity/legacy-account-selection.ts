import { z } from "zod";
import { AccountChangedError } from "./account-context.ts";

const source = z.enum(["main", "library"]);
const snapshotSchema = z.object({
  source, state: z.enum(["missing", "authorizing", "inactive", "present", "unavailable"]),
  userId: z.string().min(1).optional(), fingerprint: z.string().regex(/^[a-f\d]{64}$/u).optional(),
}).strict().superRefine((value, context) => {
  if ((value.state === "present" || value.state === "inactive") && (!value.userId || !value.fingerprint)) context.addIssue({ code: "custom", message: "Stored legacy account requires its identity and source fingerprint" });
  if (value.state === "missing" && (value.userId !== undefined || value.fingerprint !== undefined)) context.addIssue({ code: "custom", message: "Missing legacy account cannot carry an identity" });
});
export type LegacyAccountSnapshot = z.infer<typeof snapshotSchema>;
export interface LegacyAccountChoice { source: "main" | "library"; fingerprint: string }
type AccountCandidate = Readonly<{ source: "main" | "library"; userId: string; fingerprint: string; state: "present" | "inactive" }>;
export type LegacyAccountDecision =
  | { state: "signed_out" }
  | { state: "authorization_in_progress" | "unavailable"; sources: readonly ("main" | "library")[] }
  | { state: "choose_account"; candidates: readonly AccountCandidate[] }
  | { state: "verify_existing"; userId: string; candidates: readonly AccountCandidate[] }
  | { state: "reauthorize"; userId: string; source: "main" | "library" };

// 这里只决定迁移下一步。存在本地凭据不等于在线有效，也不会据此激活账号或重归属数据。
export function decideLegacyAccounts(main: LegacyAccountSnapshot, library: LegacyAccountSnapshot, choice?: LegacyAccountChoice): LegacyAccountDecision {
  const snapshots = [snapshotSchema.parse(main), snapshotSchema.parse(library)];
  if (snapshots[0]!.source !== "main" || snapshots[1]!.source !== "library") throw new Error("Legacy account sources must be distinct and ordered");
  const pending = snapshots.filter(value => value.state === "authorizing");
  if (pending.length) return { state: "authorization_in_progress", sources: pending.map(value => value.source) };
  const unavailable = snapshots.filter(value => value.state === "unavailable");
  if (unavailable.length) return { state: "unavailable", sources: unavailable.map(value => value.source) };
  const candidates: AccountCandidate[] = snapshots.flatMap(value => value.state === "present" || value.state === "inactive"
    ? [Object.freeze({ source: value.source, state: value.state, userId: value.userId!, fingerprint: value.fingerprint! })] : []);
  if (choice) {
    const parsed = z.object({ source, fingerprint: z.string().regex(/^[a-f\d]{64}$/u) }).strict().parse(choice);
    const selected = candidates.find(value => value.source === parsed.source && value.fingerprint === parsed.fingerprint);
    if (!selected) throw new AccountChangedError();
    return selected.state === "present"
      ? { state: "verify_existing", userId: selected.userId, candidates: Object.freeze([selected]) }
      : { state: "reauthorize", userId: selected.userId, source: selected.source };
  }
  if (!candidates.length) return { state: "signed_out" };
  if (new Set(candidates.map(value => value.userId)).size > 1) return { state: "choose_account", candidates: Object.freeze(candidates) };
  const active = candidates.filter(value => value.state === "present");
  if (active.length) return { state: "verify_existing", userId: active[0]!.userId, candidates: Object.freeze(active) };
  return { state: "reauthorize", userId: candidates[0]!.userId, source: candidates[0]!.source };
}
