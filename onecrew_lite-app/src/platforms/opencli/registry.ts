import { z } from "zod";
import { OpenCliError } from "./errors.ts";

const commandSchema = z.object({ command: z.string(), site: z.string(), name: z.string(), access: z.enum(["read", "write"]), browser: z.boolean(), strategy: z.string().nullable().optional() });
export type RegisteredOpenCliCommand = z.infer<typeof commandSchema>;
export class OpenCliRegistry {
  readonly #commands: readonly RegisteredOpenCliCommand[];
  constructor(value: unknown) {
    const parsed = z.array(commandSchema).min(1).safeParse(value);
    if (!parsed.success || parsed.data.some(command => command.command !== `${command.site}/${command.name}`) || new Set(parsed.data.map(command => command.command)).size !== parsed.data.length) throw new OpenCliError("opencli_invalid_output");
    this.#commands = Object.freeze(parsed.data.map(command => Object.freeze(command)));
  }
  hasBrowserRead(site: string, name: string): boolean {
    return this.#commands.some(command => command.site === site && command.name === name && command.access === "read" && command.browser);
  }
  hasPublicRead(site: string, name: string): boolean {
    return this.#commands.some(command => command.site === site && command.name === name && command.access === "read" && !command.browser && command.strategy === "public");
  }
  inventory(version: string) {
    const sites = new Map<string, { site: string; read_commands: number; write_commands: number; browser_reads: number; read_names: string[]; write_names: string[] }>();
    for (const command of this.#commands) {
      let site = sites.get(command.site);
      if (!site) { site = { site: command.site, read_commands: 0, write_commands: 0, browser_reads: 0, read_names: [], write_names: [] }; sites.set(command.site, site); }
      if (command.access === "read") { site.read_commands++; site.read_names.push(command.name); if (command.browser) site.browser_reads++; }
      else { site.write_commands++; site.write_names.push(command.name); }
    }
    const ordered = [...sites.values()].sort((left, right) => left.site < right.site ? -1 : left.site > right.site ? 1 : 0);
    for (const site of ordered) { site.read_names.sort(); site.write_names.sort(); }
    return { version, command_count: this.#commands.length, sites: ordered };
  }
}

export async function parseOpenCliJson(output: string): Promise<unknown> {
  return Promise.resolve().then(() => JSON.parse(output) as unknown).then(value => value, error => {
    if (error instanceof SyntaxError) throw new OpenCliError("opencli_invalid_output");
    throw error;
  });
}
export function validateOpenCliProfile(value: string): string {
  if (!/^[A-Za-z\d._-]{1,64}$/u.test(value)) throw new OpenCliError("opencli_invalid_input");
  return value;
}
export function parseOpenCliVersion(output: string): string {
  const version = output.trim();
  if (!version || Buffer.byteLength(version) > 64 || !/^[\d.]+$/u.test(version)) throw new OpenCliError("opencli_invalid_output");
  return version;
}
export function parseOpenCliBridgeStatus(output: string) {
  const lines = output.split(/\r?\n/u);
  const daemon = lines.find(line => line.startsWith("Daemon: "))?.slice("Daemon: ".length);
  const extension = lines.find(line => line.startsWith("Extension: "))?.slice("Extension: ".length);
  if (daemon === "not running" && extension === undefined) return { daemon_running: false, extension_connected: false };
  if (daemon === undefined || extension === undefined) throw new OpenCliError("opencli_invalid_output");
  const count = /^(\d+) profiles? connected/u.exec(extension)?.[1];
  const connected = count === undefined ? false : Number(count) <= 65535 && Number(count) > 0;
  return { daemon_running: daemon.startsWith("running"), extension_connected: extension.startsWith("connected") || connected };
}
export function parseConnectedOpenCliProfiles(output: string): { context_id: string; alias: string | null }[] {
  const profiles: { context_id: string; alias: string | null }[] = [];
  for (const line of output.split(/\r?\n/u)) {
    if (!line.startsWith("  ") || !line.includes(" — connected")) continue;
    const [context, alias] = line.slice(2).split(" — connected", 1)[0]!.trim().split(/\s+/u);
    if (!context || !/^[A-Za-z\d._-]{1,64}$/u.test(context) || alias !== undefined && alias !== "default" && !/^[A-Za-z\d._-]{1,64}$/u.test(alias)) continue;
    profiles.push({ context_id: context, alias: alias && alias !== "default" ? alias : null });
  }
  return profiles;
}
