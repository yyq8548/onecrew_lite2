import { creatorInvoke, type CreatorRecord } from "@/lib/creator";
import { parseOwnerReviewSnapshot, webResultBinding, type OwnerReviewSnapshot, type CreatorSavePreviewRow, type CreatorSaveRouteSelection } from "@/lib/owner-review";
import { z } from "zod";

const administratorSchema = z.object({
  role: z.enum(["owner", "administrator"]),
  user: z.object({
    id: z.string(), username: z.string().nullable(), first_name: z.string().nullable(),
    last_name: z.string().nullable(), is_bot: z.boolean(), verified: z.boolean(),
  }).strict(),
  eligibility: z.discriminatedUnion("status", [
    z.object({ status: z.literal("ready") }).strict(),
    z.object({ status: z.literal("blocked"),
      reason: z.enum(["not_telegram_user", "bot", "missing_public_handle", "missing_profile_url"]),
    }).strict(),
  ]),
}).strict();

const administratorPageSchema = z.object({
  community: z.object({ external_id: z.string(), title: z.string(), username: z.string().nullable(),
    kind: z.enum(["channel", "supergroup"]),
  }).strict(),
  administrators: z.array(administratorSchema),
  page_limit: z.number().int().min(0).max(65_535),
}).strict();

export type TelegramAdministrator = z.infer<typeof administratorSchema>;
export type TelegramCommunityAdministrators = z.infer<typeof administratorPageSchema>;
export type TelegramAdministratorBlockedReason = Extract<TelegramAdministrator["eligibility"], { status: "blocked" }>["reason"];
export type TelegramAdministratorSource = {
  source_creator_id: string;
  source_contact_route_id: string;
  operator_account_ref_id: string;
};
export function telegramAdministratorCandidateInput(
  administrator: TelegramAdministrator,
  community: TelegramCommunityAdministrators["community"],
  source: TelegramAdministratorSource,
) {
  return {
    target: { platform: "telegram_administrator" as const, user_id: administrator.user.id,
      community: { kind: "saved_community_route" as const, creator_id: source.source_creator_id, route_id: source.source_contact_route_id } },
    connection: { platform: "telegram" as const, account_ref_id: source.operator_account_ref_id },
    provenance: { kind: "administrator_selection" as const, community_id: community.external_id,
      community_kind: community.kind, source: { kind: "saved_community_route" as const,
        creator_id: source.source_creator_id, route_id: source.source_contact_route_id } },
    notes: null,
    tags: null,
    quality: { score: null, assessment: null, follower_counts: {} },
  };
}

export function creatorSaveReviewRows(snapshot: OwnerReviewSnapshot): CreatorSavePreviewRow[] {
  return snapshot.state.kind === "available" && snapshot.pending_action?.kind === "creator_save_batch"
    ? snapshot.pending_action.review.rows : [];
}

export function creatorSaveSelections(rows: CreatorSavePreviewRow[], routeIds: Record<string, string[]>): CreatorSaveRouteSelection[] {
  return rows.map(({ row_id, preview }) => ({ row_id, confirmed_route_ids: preview.routes
    .filter((route) => route.collision.status === "none" && (route.required || routeIds[row_id]?.includes(route.route_id))).map((route) => route.route_id) }));
}

export function creatorSaveReviewBlocked(rows: CreatorSavePreviewRow[]): boolean {
  return rows.some(({ preview }) => preview.routes.some((route) => route.required && route.collision.status !== "none"));
}

export function savedTelegramCreatorId(snapshot: OwnerReviewSnapshot): string | null {
  if (snapshot.state.kind !== "resolved") return null;
  const outcomes = snapshot.presentation.sections.flatMap((section) => section.kind === "creator_save_rows" ? section.rows : []);
  const saved = outcomes.filter((row) => row.status === "saved" && row.creator_id);
  return saved.length === 1 ? saved[0].creator_id : null;
}

export async function readTelegramCommunityAdministrators(source: TelegramAdministratorSource): Promise<TelegramCommunityAdministrators> {
  const response = await creatorInvoke<unknown>("onecrew_telegram_community_administrators_read", { input: source });
  const parsed = administratorPageSchema.safeParse(response);
  if (!parsed.success) throw new Error("telegram_administrator_response_invalid");
  return parsed.data;
}

export function prepareTelegramAdministratorSave(administrator: TelegramAdministrator, community: TelegramCommunityAdministrators["community"], source: TelegramAdministratorSource) {
  return creatorInvoke<unknown>("onecrew_creator_save_prepare", {
    input: { candidates: [telegramAdministratorCandidateInput(administrator, community, source)] },
  }).then(parseOwnerReviewSnapshot);
}

export function confirmTelegramAdministratorSave(snapshot: OwnerReviewSnapshot, selections: CreatorSaveRouteSelection[]) {
  return creatorInvoke<unknown>("onecrew_creator_save_confirm", { input: { binding: webResultBinding(snapshot.binding), selections } }).then(parseOwnerReviewSnapshot);
}

export function readCreatorSaveResult(snapshot: OwnerReviewSnapshot) {
  return creatorInvoke<unknown>("onecrew_creator_save_result_get", { input: webResultBinding(snapshot.binding) }).then(parseOwnerReviewSnapshot);
}

export function readSavedTelegramCreator(creatorId: string) {
  return creatorInvoke<CreatorRecord>("onecrew_creator_get", { input: { creator_id: creatorId } });
}
