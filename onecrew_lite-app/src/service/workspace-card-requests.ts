import { z } from "zod";

export const workspaceCardRequest = z.strictObject({ view_id: z.uuid(), manual: z.boolean().default(false), baseline_campaign_id: z.uuid().nullish().transform(value => value ?? null) });
export type WorkspaceCardRequest = z.input<typeof workspaceCardRequest>;
export type WorkspaceCardReply = { kind: "none" } | { kind: "message"; message: string };
interface View { account: string | null; baseline: string | null; registered: number; seen: number | null; recovered: boolean }
interface Start { at: number; claimed: boolean; claimedBy: string | null }
const key = (account: string, campaign: string) => JSON.stringify([account, campaign]);
function displayMessage(chinese: boolean): WorkspaceCardReply {
  return { kind: "message", message: chinese
    ? "请调用 onecrew_status，然后调用 onecrew_web，在当前聊天底部展示 OneCrew 工作台最新的只读状态卡。仅展示状态，不打开或切换浏览器页面；不要启动、恢复或批准搜索，不要修改任务条件，不要重复发送此请求。"
    : "Call onecrew_status, then onecrew_web to display the latest read-only OneCrew status card at the bottom of this chat. Display only: do not open or switch browser pages, start, resume or approve searches, change conditions, or resend this request." };
}
// 仅保存显示通知。领取发生在传输之前，不以不确定的宿主响应为理由重复发送。
export class WorkspaceCardRequests {
  readonly #views = new Map<string, View>(); readonly #starts = new Map<string, Start>(); readonly #retired = new Set<string>();
  register(id: string, account: string | null, baseline: string | null): void {
    z.uuid().parse(id);
    if (this.#views.size >= 256) this.#views.clear();
    const start = account === null || baseline === null ? undefined : this.#starts.get(key(account, baseline));
    if (start) {
      start.claimed = true;
      if (start.claimedBy !== null) { this.#views.delete(start.claimedBy); this.#retired.add(start.claimedBy); start.claimedBy = null; }
    }
    this.#views.set(id, { account, baseline, registered: performance.now(), seen: null, recovered: false });
  }
  started(account: string, campaign: string): void {
    if (this.#starts.size >= 4096) return;
    const id = key(account, campaign);
    if (!this.#starts.has(id)) this.#starts.set(id, { at: performance.now(), claimed: false, claimedBy: null });
  }
  request(raw: WorkspaceCardRequest, account: string, campaign: string, chinese: boolean): WorkspaceCardReply {
    const request = workspaceCardRequest.parse(raw);
    if (this.#retired.has(request.view_id) && !request.manual) return { kind: "none" };
    if (request.manual) this.#retired.delete(request.view_id);
    if (!this.#views.has(request.view_id) && this.#views.size < 256) this.#views.set(request.view_id, { account, baseline: request.baseline_campaign_id, registered: performance.now(), seen: null, recovered: true });
    if (request.manual) {
      const view = this.#views.get(request.view_id);
      if (view && view.account === null) view.account = account;
      if (view?.account !== account) return { kind: "none" };
      const start = this.#starts.get(key(account, campaign));
      if (start) { start.claimed = true; start.claimedBy = request.view_id; }
      return displayMessage(chinese);
    }
    return this.claim(request.view_id, account, campaign, chinese);
  }
  claim(id: string, account: string, campaign: string, chinese: boolean): WorkspaceCardReply {
    const view = this.#views.get(id), now = performance.now();
    if (!view) return { kind: "none" };
    view.account ??= account;
    if (view.account !== account) return { kind: "none" };
    const previous = view.seen; view.seen = now;
    if (previous === null || now - previous > 8_000 || view.baseline === campaign) return { kind: "none" };
    const visible = [...this.#views.values()].filter(value => value.account === account && value.seen !== null && now - value.seen <= 8_000).length;
    if (visible !== 1) return { kind: "none" };
    const start = this.#starts.get(key(account, campaign));
    if (!start || start.claimed || start.at <= view.registered && !view.recovered) return { kind: "none" };
    start.claimed = true; start.claimedBy = id;
    return displayMessage(chinese);
  }
}
