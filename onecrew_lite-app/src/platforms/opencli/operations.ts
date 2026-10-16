import { z } from "zod";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import type { OperationRegistry } from "../../application/operation-registry.ts";
import type { AccountBrowserBindingResolver } from "../browser/binding.ts";
import type { OpenCliAddons } from "./addons.ts";
import { OpenCliReader } from "./reader.ts";
import { resolveOpenCliInstallation, VERIFIED_OPENCLI_VERSION } from "./installation.ts";
import { OpenCliError } from "./errors.ts";
import { repairManagedOpenCli } from "./managed-install.ts";
import { openChromeSetupPage } from "../browser/chrome-host.ts";

export interface OpenCliOperationDependencies { addons(context: AccountContext): Promise<OpenCliAddons> }
export function registerOpenCliOperations(registry: OperationRegistry, accounts: AccountContextService, bindings: AccountBrowserBindingResolver, dependencies: OpenCliOperationDependencies) {
  async function read(context: AccountContext) {
    const addons = await dependencies.addons(context), installation = await resolveOpenCliInstallation(), reader = await OpenCliReader.inspect(addons, installation, context.signal);
    accounts.assertCurrent(context); return { reader, addons, installation };
  }
  async function status(context: AccountContext) {
    const addons = await dependencies.addons(context);
    const inspected = await read(context).then(value => ({ value }), error => ({ error }));
    if ("error" in inspected) {
      if (!(inspected.error instanceof OpenCliError) || inspected.error.code !== "opencli_unavailable") throw inspected.error;
      return { installed: false, version: null, required_version: VERIFIED_OPENCLI_VERSION, command_path: null, command_count: null, daemon_running: false, extension_connected: false, profiles: [], search_capabilities: [], addons: await addons.status(), issue: "not_installed" };
    }
    const { reader, installation } = inspected.value;
    const bridge = await reader.bridgeStatus(), profiles = bridge.daemon_running && bridge.extension_connected ? await reader.connectedProfiles() : [];
    const result = { installed: true, version: reader.version, required_version: VERIFIED_OPENCLI_VERSION, command_path: installation.script ?? installation.executable, command_count: reader.inventory().command_count, ...bridge, profiles, search_capabilities: await reader.creatorSearchCapabilities(), addons: await addons.status(), issue: !bridge.daemon_running ? "daemon_unavailable" : !bridge.extension_connected ? "extension_unavailable" : null };
    accounts.assertCurrent(context); return result;
  }
  const observations = new Map<string, Map<string, Awaited<ReturnType<OpenCliReader["checkCreatorAuthStatus"]>>>>();
  const key = (context: AccountContext, profile: string) => JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId, context.principal.deviceId, context.epoch, profile]);
  registry.register("onecrew_opencli_setup_status", z.strictObject({}), async (_, context) => status(context));
  registry.register("onecrew_opencli_setup_page_open", z.strictObject({ page: z.enum(["download", "extension"]) }), async (input, context) => { accounts.assertCurrent(context); await openChromeSetupPage(input.page); return { page: input.page, browser: "chrome" as const, launch_requested: true as const }; });
  registry.register("onecrew_opencli_login_page_open", z.strictObject({ profile: z.string().min(1), platform: z.string().min(1) }), async (input, context) => { const environment = await read(context), binding = await bindings.resolveProfileRef(context, input.profile); return environment.reader.openCreatorLoginPage(binding, input.platform); });
  registry.register("onecrew_opencli_bridge_recover", z.strictObject({}), async (_, context) => { const environment = await read(context); await environment.reader.recoverLocalBridge({ signal: context.signal, assertCurrent: async () => accounts.assertCurrent(context) }); return status(context); });
  registry.register("onecrew_opencli_profile_check", z.strictObject({ profile: z.string().min(1) }), async (input, context) => {
    const environment = await read(context), binding = await bindings.resolveProfileRef(context, input.profile); await environment.reader.checkBrowserProfile(binding); accounts.assertCurrent(context); return { profile: input.profile, preflight_passed: true as const };
  });
  registry.register("onecrew_opencli_addons_install", z.strictObject({ approved: z.literal(true), repair: z.boolean().optional() }), async (input, context) => {
    const environment = await read(context); const result = input.repair === true ? await environment.addons.repair(true) : await environment.addons.install(true); accounts.assertCurrent(context); return result;
  });
  registry.register("onecrew_opencli_installation_repair", z.strictObject({ approved: z.literal(true) }), async (_, context) => {
    await repairManagedOpenCli(true); accounts.assertCurrent(context); return status(context);
  });
  registry.register("onecrew_opencli_auth_observations", z.strictObject({ profile: z.string().min(1) }), async (input, context) => {
    await bindings.resolveProfileRef(context, input.profile); accounts.assertCurrent(context); return [...(observations.get(key(context, input.profile))?.values() ?? [])];
  });
  registry.register("onecrew_opencli_site_auth_check", z.strictObject({ profile: z.string().min(1), platform: z.string().min(1) }), async (input, context) => {
    const environment = await read(context), binding = await bindings.resolveProfileRef(context, input.profile), observation = await environment.reader.checkCreatorAuthStatus(binding, input.platform);
    accounts.assertCurrent(context); const id = key(context, input.profile), values = observations.get(id) ?? new Map(); values.set(input.platform, observation); observations.set(id, values); return observation;
  });
  return { status, close() { observations.clear(); } };
}
