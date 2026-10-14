/** Display labels only; keep provider keys unchanged in requests and saved data. */
export const platformDisplayName = (platform: string) => platform.toLowerCase() === "twitter" ? "X" : platform;
