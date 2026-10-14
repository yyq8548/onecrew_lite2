import type { CreatorSaveBatchOutcome, CreatorSaveBatchReview, ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { creatorSaveSnapshot, CREATOR_SAVE_PREVIEW } from "./creator-save-fixture";

export function creatorBatchReview(count = 5): CreatorSaveBatchReview {
  const assessments = ["已检查公开主页；内容主题包含日常妆容、护肤步骤和产品试用。当前评分为 0，是示例低分，并非未评分。需进一步核对近三十天更新频率及合作邮箱来源。", "以敏感肌护理和成分讨论为主，受众方向与本次需求接近。建议在保存后确认最近内容仍围绕护肤，而非仅依赖过往热门视频。", "主页包含完整化妆教学及产品对比，内容表达清晰。公开邮箱来自简介，可作为后续人工核验的线索；当前不发送消息。", "短视频更新稳定，主要内容覆盖轻妆和日常穿搭。平台主页应始终保留；外部网站和邮箱属于可选联系方式。", null];
  const names = ["Dena Beauty", "River Skin", "Mia Makeup", "Emma Style", "Sike Cosmetics", "Lena Daily", "BTS Makeup", "Olive Life"];
  return { intent: "save_or_update", summary: { total: count, eligible_new: count, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: 0 }, failures: [], rows: Array.from({ length: count }, (_, index) => ({ row_id: `save.creator.${index + 1}`, preview: {
    ...structuredClone(CREATOR_SAVE_PREVIEW), display_name: names[index % names.length]!, handle: `creator_${index + 1}`,
    platform: ["x", "instagram", "youtube", "tiktok", "x"][index % 5] as "x" | "instagram" | "youtube" | "tiktok",
    profile_url: `https://${["x", "instagram", "youtube", "tiktok", "x"][index % 5]}.example.test/creator_${index + 1}`,
    routes: [...CREATOR_SAVE_PREVIEW.routes.map(route=>route.route_id === "route-profile" ? {...structuredClone(route),channel:["x", "instagram", "youtube", "tiktok", "x"][index % 5] as "x" | "instagram" | "youtube" | "tiktok",display_value:`@creator_${index + 1}`,normalized_value:`https://${["x", "instagram", "youtube", "tiktok", "x"][index % 5]}.example.test/creator_${index + 1}`} : {...structuredClone(route),display_value:route.channel === "website" ? `https://creator-${index + 1}.example.test` : `creator_${index + 1}@example.test`,normalized_value:route.channel === "website" ? `https://creator-${index + 1}.example.test/` : `creator_${index + 1}@example.test`}), ...(index === 0 ? (["instagram", "youtube", "tiktok", "telegram"] as const).map(channel=>({...structuredClone(CREATOR_SAVE_PREVIEW.routes[0]!), route_id:`route-${channel}`,channel,required:false,display_value:"@creator_1",normalized_value:`https://${channel}.example.test/creator_1`})) : [])],
    assessment: assessments[index % assessments.length]!,
    score: index === 0 ? 0 : index === 4 ? null : 45 + (index % 5) * 12, follower_counts: { x: index === 0 ? 0 : (index + 1) * 25000 },
  } })) };
}
export function creatorBatchSnapshot(count = 5): ResultSnapshot {
  const base = creatorSaveSnapshot(); const review = creatorBatchReview(count);
  return { ...base, presentation: { kind: "creator_save_batch_confirmation", title: "审核 Creator 列表", summary: "选择需要保存的 Creator。", sections: [] }, pending_action: { kind: "creator_save_batch", review },
    batch: { outcomes: review.rows.map(row => ({ call_id: row.row_id, operation_id: "onecrew.creator.candidate.save", disposition: { kind: "not_run", reason: "confirmation_required" }, summary: "Review Creator save", resource_refs: [] })) },
  };
}
export function creatorBatchOutcomeRows(statuses: CreatorSaveBatchOutcome["status"][] = ["saved", "failed", "unknown", "not_run", "not_selected"]): CreatorSaveBatchOutcome[] {
  return creatorBatchReview(statuses.length).rows.map(({ row_id, preview }, index) => ({ row_id, display_name: preview.display_name, platform: preview.platform, handle: preview.handle, profile_url: preview.profile_url, avatar_url: preview.avatar_url,
    status: statuses[index]!, operation: statuses[index] === "saved" ? "added" : statuses[index] === "failed" ? "failed" : statuses[index] === "unknown" ? "unknown" : statuses[index] === "not_selected" ? "not_selected" : "failed", reason: statuses[index] === "failed" ? "row_failed" : statuses[index] === "unknown" ? "commit_unknown" : statuses[index] === "not_selected" ? "user_not_selected" : statuses[index] === "not_run" ? "preparation_failed" : null, creator_id: statuses[index] === "saved" ? `creator-saved-${index + 1}` : null, failure_code: statuses[index] === "failed" ? "save_failed" : statuses[index] === "unknown" ? "save_unknown" : null, confirmed_route_ids: statuses[index] === "not_selected" ? [] : ["route-profile"],
  }));
}
export function creatorBatchResultSnapshot(statuses?: CreatorSaveBatchOutcome["status"][]): ResultSnapshot {
  const rows = creatorBatchOutcomeRows(statuses); const base = creatorBatchSnapshot(rows.length);
  const unknown = rows.some(row => row.status === "unknown"); const partial = rows.some(row => row.status === "failed" || row.status === "not_run");
  return { ...base, pending_action: null, completed_sections_count: 1, binding: { ...base.binding, revision: base.binding.revision + 1 },
    presentation: { ...base.presentation, kind: "creator_save_batch_outcome", sections: [{ kind: "creator_save_rows", rows }] },
    state: unknown ? { kind: "unknown", code: "creator_save_commit_unknown", summary: "存在未知结果，请逐项核对。" } : partial ? { kind: "failed", code: "creator_save_partial_failure", summary: "部分 Creator 未能保存。", retryable: false } : { kind: "resolved", code: "creator_save_completed", summary: "所选 Creator 已保存。" },
    batch: { outcomes: rows.map(row => ({ call_id: row.row_id, operation_id: "onecrew.creator.candidate.save", summary: row.status, resource_refs: row.creator_id ? [{ kind: "creator", id: row.creator_id, label: row.display_name }] : [], disposition: row.status === "saved" ? { kind: "succeeded" } : row.status === "failed" || row.status === "unknown" ? { kind: row.status, code: row.failure_code! } : { kind: "not_run", reason: row.status } })) },
  };
}
