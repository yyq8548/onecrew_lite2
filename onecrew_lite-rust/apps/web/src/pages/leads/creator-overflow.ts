// 列表里靠「数量」隐藏的内容:+N 平台、省略的通道、+N 粉丝量。这三处和
// truncate 不一样 —— truncate 只裁像素,完整文本节点还在 DOM 里;这里是
// 直接不渲染,读屏也拿不到。所以隐藏了什么必须能算出来,才能补进 tooltip
// 和 sr-only。

import { PLATFORM_LABEL, formatCount, type CreatorSummary, type CreatorPlatform } from "@/lib/creator";

/** 首个平台之外的其余平台。`platforms` 与 `primary_platform` 同源同序
 *  (仓库侧同一套 state != 'rejected' 过滤 + 首次收录排序),所以直接切掉
 *  第一个就是「被 +N 收起来的那些」。 */
export function hiddenPlatformLabels(summary: CreatorSummary): string[] {
  return summary.platforms.slice(1).map((platform) => PLATFORM_LABEL[platform]);
}

/** 超出前 `shown` 个的通道。 */
export function hiddenChannels<T>(channels: T[], shown: number): T[] {
  return channels.slice(shown);
}

/**
 * 除了正在显示的那个平台之外,还有粉丝量记录的平台。
 *
 * 不能简单地「去掉第一个」:follower_counts 是 Rust 侧的 BTreeMap,JSON 里
 * 的键序是 Platform 的字典序,不是收录顺序,而 primaryFollowerEntry 挑的是
 * primary_platform(或 platforms 里第一个有数的)。按键序切会漏报也会重报,
 * 必须按「实际显示的那个平台」排除。
 */
export function hiddenFollowerCounts(
  summary: CreatorSummary,
  shownPlatform: CreatorPlatform,
  locale?: string,
): string[] {
  const counts = (summary.follower_counts ?? {}) as Partial<Record<CreatorPlatform, number>>;
  return (Object.entries(counts) as [CreatorPlatform, number][])
    .filter(([platform]) => platform !== shownPlatform)
    .map(([platform, count]) => `${PLATFORM_LABEL[platform]} ${formatCount(count, locale)}`);
}
