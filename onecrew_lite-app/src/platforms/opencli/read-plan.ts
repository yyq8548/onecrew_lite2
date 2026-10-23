import type { OpenCliAddonId } from "./addons.ts";

// 只由各平台固定参数构造器创建；不提供接受计划或任意 argv 的公开门面。
export interface ReviewedOpenCliRead<T> {
  readonly site: string;
  readonly name: string;
  readonly args: readonly string[];
  readonly addons: readonly OpenCliAddonId[];
  readonly maximumBytes?: number;
  readonly public?: boolean;
  readonly foregroundBrowser?: boolean;
  readonly emptyOn66?: () => T;
  decode(value: unknown): T;
}
