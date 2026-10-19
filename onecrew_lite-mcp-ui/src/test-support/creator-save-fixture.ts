import type { CreatorSavePreview, ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

export const CREATOR_SAVE_PREVIEW: CreatorSavePreview = {
  platform: "x", display_name: "Demo beauty creator", handle: "@beauty_demo",
  profile_url: "https://x.example.test/beauty_demo", source_url: null, avatar_url: null, identity_status: "new", identity_creator_ids: [], intent: "save_or_update", status: "eligible_new", reason: null,
  notes: "美妆与护肤创作者，已查看公开主页。", score: 0, assessment: "尚需核对后续合作需求。", follower_counts: { followers: 12500 }, expires_at: 1900000000,
  routes: [
    { route_id: "route-profile", channel: "x", kind: "profile", display_value: "@beauty_demo", normalized_value: "https://x.example.test/beauty_demo", sources: ["public profile"], required: true, collision: { status: "none", creator_ids: [] } },
    { route_id: "route-email", channel: "email", kind: "email", display_value: "creator@example.test", normalized_value: "creator@example.test", sources: ["profile bio"], required: false, collision: { status: "possible", creator_ids: ["creator-existing"] } },
    { route_id: "route-site", channel: "website", kind: "url", display_value: "https://creator.example.test", normalized_value: "https://creator.example.test/", sources: [], required: false, collision: { status: "none", creator_ids: [] } },
  ],
};

export function creatorSaveSnapshot(preview = CREATOR_SAVE_PREVIEW): ResultSnapshot {
  const summary = preview.status === "eligible_new"
    ? { total: 1, eligible_new: 1, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: 0 }
    : preview.status === "eligible_update"
      ? { total: 1, eligible_new: 0, eligible_update: 1, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: 0 }
      : preview.status === "existing_skipped"
        ? { total: 1, eligible_new: 0, eligible_update: 0, existing_skipped: 1, duplicate_skipped: 0, blocked: 0, failed: 0 }
        : preview.status === "duplicate_skipped"
          ? { total: 1, eligible_new: 0, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 1, blocked: 0, failed: 0 }
          : { total: 1, eligible_new: 0, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: preview.status === "failed" ? 0 : 1, failed: preview.status === "failed" ? 1 : 0 };
  return { ...SNAPSHOT, binding: structuredClone(SNAPSHOT.binding), batch: { outcomes: [{ call_id: "save.creator", operation_id: "onecrew.creator.candidate.save", disposition: { kind: "not_run", reason: "confirmation_required" }, summary: "Review Creator save", resource_refs: [] }] },
    presentation: { kind: "creator_save_batch_confirmation", title: "Review Creator save", summary: "Select routes and confirm", sections: [] },
    pending_action: { kind: "creator_save_batch", review: { rows: [{ row_id: "save.creator.1", preview: structuredClone(preview) }], failures: [], intent: "save_or_update", summary } },
  };
}

export function creatorSavedSnapshot(): ResultSnapshot {
  const current = creatorSaveSnapshot();
  current.binding.revision++;
  return { ...current, binding: { ...current.binding }, pending_action: null, completed_sections_count: 1,
    batch: { outcomes: [{ ...current.batch.outcomes[0]!, disposition: { kind: "succeeded" }, resource_refs: [{ kind: "creator", id: "creator-demo", label: "Demo beauty creator" }] }] },
    state: { kind: "resolved", code: "creator_save_completed", summary: "Creator saved to OneCrew." },
    presentation: { kind: "creator_save_batch_outcome", title: "Creator saved", summary: "Creator saved to OneCrew.", sections: [{kind:"creator_save_rows",rows:[{row_id:"save.creator.1",display_name:CREATOR_SAVE_PREVIEW.display_name,platform:CREATOR_SAVE_PREVIEW.platform,handle:CREATOR_SAVE_PREVIEW.handle,profile_url:CREATOR_SAVE_PREVIEW.profile_url,avatar_url:CREATOR_SAVE_PREVIEW.avatar_url,status:"saved",operation:"added",reason:null,creator_id:"creator-demo",failure_code:null,confirmed_route_ids:["route-profile"]}]}] },
  };
}
