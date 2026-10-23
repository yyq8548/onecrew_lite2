import { BlockList, isIP } from "node:net";
import { lookup } from "node:dns/promises";
import { z } from "zod";
import { OpenCliError } from "./errors.ts";

const sensitive = new Set(["access_token", "refresh_token", "id_token", "api_key", "password", "authorization"]);
export function publicWebUrl(value: string, allowPublishedHttp = false): URL | null {
  if (!value.isWellFormed() || Buffer.byteLength(value) > 2048 || /[\p{Cc}\p{White_Space}]/u.test(value) || value.includes("...") || value.includes("…") || !z.url().safeParse(value).success) return null;
  const url = new URL(value);
  const host = url.hostname;
  if (!(url.protocol === "https:" || allowPublishedHttp && url.protocol === "http:") || url.username || url.password || url.port || [...url.searchParams.keys(), ...new URLSearchParams(url.hash.slice(1)).keys()].some(key => sensitive.has(key.toLowerCase())) || isIP(host.replace(/^\[|\]$/gu, "")) !== 0 || !host.includes(".") || host.endsWith(".") || ["localhost", "local", "internal", "invalid", "test", "onion", "home.arpa"].some(suffix => host === suffix || host.endsWith(`.${suffix}`))) return null;
  return url;
}
export function validatePublicWebUrl(value: string): URL {
  const url = publicWebUrl(value);
  if (!url) throw new OpenCliError("opencli_invalid_input");
  return url;
}
const privateV4 = new BlockList();
for (const [address, prefix] of [["10.0.0.0", 8], ["172.16.0.0", 12], ["192.168.0.0", 16], ["127.0.0.0", 8], ["169.254.0.0", 16], ["192.0.2.0", 24], ["198.51.100.0", 24], ["203.0.113.0", 24], ["0.0.0.0", 8], ["224.0.0.0", 3], ["100.64.0.0", 10], ["198.18.0.0", 15], ["192.0.0.0", 16]] as const) privateV4.addSubnet(address, prefix, "ipv4");
const globalV6 = new BlockList(); globalV6.addSubnet("2000::", 3, "ipv6");
const reservedV6 = new BlockList();
for (const [address, prefix] of [["2001::", 23], ["2001:db8::", 32], ["2002::", 16], ["3fff::", 20]] as const) reservedV6.addSubnet(address, prefix, "ipv6");
export function publicOpenCliAddress(address: string): boolean {
  const family = isIP(address);
  return family === 4 ? !privateV4.check(address, "ipv4") : family === 6 ? globalV6.check(address, "ipv6") && !reservedV6.check(address, "ipv6") : false;
}
export async function checkPublicWebDestination(value: string, signal?: AbortSignal): Promise<void> {
  const url = validatePublicWebUrl(value);
  signal?.throwIfAborted();
  let deadline: ReturnType<typeof setTimeout>;
  let cancel: () => void;
  const stopped = new Promise<never>((_, reject) => {
    deadline = setTimeout(() => reject(new OpenCliError("opencli_timeout")), 5000);
    cancel = () => reject(new OpenCliError("opencli_cancelled"));
    signal?.addEventListener("abort", cancel, { once: true });
  });
  try {
    const addresses = await Promise.race([lookup(url.hostname, { all: true }).then(value => value, () => { throw new OpenCliError("opencli_invalid_input"); }), stopped]);
    signal?.throwIfAborted();
    if (!addresses.length || addresses.some(address => !publicOpenCliAddress(address.address))) throw new OpenCliError("opencli_invalid_input");
  } finally { clearTimeout(deadline!); signal?.removeEventListener("abort", cancel!); }
}
