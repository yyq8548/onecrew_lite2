import { NodeCryptoProvider } from "@mtcute/node/utils.js";

let readiness: Promise<void> | undefined;
// 仅加载并验证本地加密实现及 Wasm；此检查不创建 Telegram 连接。
export function assertNativeTelegramCryptography(): Promise<void> {
  readiness ??= new NodeCryptoProvider().initialize();
  return readiness;
}
