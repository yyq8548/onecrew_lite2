// 历史私信记录的平台标识，不授予发送能力；当前对外发送仅支持邮件页 Gmail。

import type { Platform } from "@/types";

/** 每个带私聊收件箱的平台,按展示顺序。 */
export const DM_PLATFORMS = ["x", "instagram", "telegram"] as const satisfies readonly Platform[];

/** 一条建联通道可能是邮箱、网站或聚合链接,这里判定它是否能承载会话。 */
export function isDmPlatform(channel: string): channel is Platform {
  return (DM_PLATFORMS as readonly string[]).includes(channel);
}
