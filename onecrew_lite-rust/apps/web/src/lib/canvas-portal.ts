// 模态该盖住哪一块。
//
// Radix 的 Portal 默认落在 document.body,于是遮罩的 fixed inset-0 相对视口
// 铺满整个工作区,让模态遮罩覆盖应用外壳而不是只覆盖内容卡片。
// 的内容,却一起被压黑。把 Portal 指到画布容器上,遮罩就只盖画布。
//
// 这只改「画在哪」,不改模态语义 —— modal 仍然会在 document.body 上设
// pointer-events: none 并对画布之外的子树打 aria-hidden。真正的模态(销毁性
// 确认、凭据输入、外发确认)就该这样:答案没给之前背景不该发生交互。
//
// Toolkit 与 Creator 详情虽然从右侧滑入,仍然是模态:遮罩、外部点击关闭、
// Escape、焦点陷阱与焦点返回都交给 Radix。container 只限定它们在画布内
// 的视觉范围,不把它们降级成可同时操作背景的非模态面板。

import { createContext, useContext } from "react";

const CanvasPortalContext = createContext<HTMLElement | null>(null);

export const CanvasPortalProvider = CanvasPortalContext.Provider;

/** 画布容器。模态要用它当 Portal 目标。 */
export function useCanvasPortal(): HTMLElement | null {
  return useContext(CanvasPortalContext);
}
