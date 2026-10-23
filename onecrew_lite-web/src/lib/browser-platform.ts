/** Browser keyboard conventions; this does not grant a native capability. */
export type BrowserPlatform = "macos" | "windows" | "linux" | "unknown";

export function currentBrowserPlatform(): BrowserPlatform {
  if (typeof navigator === "undefined") return "unknown";
  const platform = navigator.platform.toLowerCase();
  if (platform.includes("mac")) return "macos";
  if (platform.includes("win")) return "windows";
  if (platform.includes("linux")) return "linux";
  return "unknown";
}
