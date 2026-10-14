// 有私聊收件箱的平台,只在这里列一次。
//
// 私聊要从一个操作账号发出去,而 OneCrew 能连的操作账号恰好就是 X /
// Instagram / Telegram —— YouTube 走服务凭据,TikTok 没有本地连接,Creator
// runtime 直接拒绝它们的私聊(`crates/creator-runtime/src/outreach/shared.rs`
// 的 `message_channel`)。所以「能私聊的平台」和「能连操作账号的平台」是同一个
// 集合,`Platform` 就是它;这里不再另起一个同值的联合类型。

import type { Platform } from "@/types";

/** 每个带私聊收件箱的平台,按展示顺序。 */
export const DM_PLATFORMS = ["x", "instagram", "telegram"] as const satisfies readonly Platform[];

/** 一条建联通道可能是邮箱、网站或聚合链接,这里判定它是否能承载会话。 */
export function isDmPlatform(channel: string): channel is Platform {
  return (DM_PLATFORMS as readonly string[]).includes(channel);
}
