export function parseCardRequest(value: unknown): { kind: "none" } | { kind: "message"; message: string } {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid card request");
  const data = value as Record<string, unknown>;
  if (data.kind === "none" && Object.keys(data).length === 1) return { kind: "none" };
  if (data.kind === "message" && Object.keys(data).sort().join() === "kind,message"
    && typeof data.message === "string" && data.message.length > 0 && data.message.length <= 2048) {
    return { kind: "message", message: data.message };
  }
  throw new Error("invalid card request");
}
