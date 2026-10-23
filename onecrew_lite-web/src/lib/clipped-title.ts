// CSS 截断只裁像素:完整文本节点还在 DOM 里,读屏照样念得到 —— 丢信息的
// 只有用鼠标的明眼人。所以在 hover 当刻量一次就够:不挂 ResizeObserver、
// 不进 state、不触发重渲染。列宽是流动的(表格按 size/minSize 收缩),
// 提前算「会不会截」没有意义。
//
// 代价:给一个已经在无障碍树里的元素加 title,某些读屏会念两遍。这是取舍。

import type { PointerEvent } from "react";

/** scrollWidth / clientWidth 都取整,留 1px 容差 —— 亚像素溢出会给完全
 *  看得见的文字挂上 tooltip。 */
export function isClipped(element: { scrollWidth: number; clientWidth: number }): boolean {
  return element.scrollWidth - element.clientWidth > 1;
}

/** 挂在带 `truncate` 的那个元素上:`onPointerEnter={revealWhenClipped}`。 */
export function revealWhenClipped(event: PointerEvent<HTMLElement>): void {
  const element = event.currentTarget;
  const full = element.textContent ?? "";
  if (full && isClipped(element)) element.title = full;
  else element.removeAttribute("title");
}
