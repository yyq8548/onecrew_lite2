import { homedir } from "node:os";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { VerifiedMailIdentityProvider } from "../mail/gmail-access.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { AccountBrowserBindingResolver, type BrowserAccountDirectoryResolver, type TrustedExtensionContextProofVerifier } from "../platforms/browser/binding.ts";
import type { PlatformVaultReader } from "../platforms/browser/platform-vault.ts";
import { OpenCliAddons } from "../platforms/opencli/addons.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { OpenCliReader } from "../platforms/opencli/reader.ts";
import type { SemanticStructuredClient } from "./semantic-client.ts";
import { HostSemanticClient, type HostWorkNotice } from "./host-work.ts";
import type { DiscoveryBrowserResolver } from "./search-execution.ts";

export interface DiscoveryReadEnvironment { readonly reader: OpenCliReader; readonly browsers: DiscoveryBrowserResolver }
export interface DiscoveryDependencyResolver { reads(context: AccountContext): Promise<DiscoveryReadEnvironment>; semantic(context: AccountContext): Promise<SemanticStructuredClient>; stopSemantic?(): void }
export interface TrustedDiscoveryConfiguration {
  openCliHome?(context: AccountContext): Promise<string>;
  accounts: AccountContextService;
  identities: VerifiedMailIdentityProvider;
  directories: BrowserAccountDirectoryResolver;
  browsers?: DiscoveryBrowserResolver;
  vault?: PlatformVaultReader;
  extensionProofs?: TrustedExtensionContextProofVerifier;
  onHostWork?(context: AccountContext, notice: HostWorkNotice, active: () => boolean): (cancelled?: boolean) => void;
}
// 创建工厂不探测CLI、读取凭据、安装扩展或打开浏览器。配置只由受信主程序提供。
export function createDefaultDiscoveryDependencies(configuration: TrustedDiscoveryConfiguration): DiscoveryDependencyResolver {
  const semanticClients = new WeakMap<AccountContext, Promise<HostSemanticClient>>(), shutdown = new AbortController();
  return Object.freeze({
    stopSemantic() { shutdown.abort(new Error("discovery_semantic_stopping")); },
    async reads(context: AccountContext): Promise<DiscoveryReadEnvironment> {
      configuration.accounts.assertCurrent(context);
      let browsers = configuration.browsers;
      if (!browsers) {
        if (!configuration.vault || !configuration.extensionProofs) throw new OpenCliError("opencli_profile_unavailable");
        const bindings = new AccountBrowserBindingResolver(configuration.accounts, configuration.identities, configuration.directories, configuration.vault, configuration.extensionProofs);
        browsers = Object.freeze({ resolve: (owner: AccountContext, profileRef: string) => bindings.resolveProfileRef(owner, profileRef) });
      }
      const home = configuration.openCliHome ? await configuration.openCliHome(context) : homedir();
      configuration.accounts.assertCurrent(context);
      const addons = new OpenCliAddons(runtimeAssetPath("opencli-addons", new URL("../../../onecrew_lite-opencli", import.meta.url)), home);
      const reader = await OpenCliReader.inspect(addons, undefined, context.signal);
      configuration.accounts.assertCurrent(context);
      return Object.freeze({ reader, browsers });
    },
    async semantic(context: AccountContext): Promise<SemanticStructuredClient> {
      configuration.accounts.assertCurrent(context);
      let client = semanticClients.get(context);
      if (!client) {
      client = (async () => {
      const owned = await configuration.directories.accountDirectory(context); owned.assertOwned();
      if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_semantic_directory_owner_mismatch");
      configuration.accounts.assertCurrent(context); owned.assertOwned();
      return new HostSemanticClient(owned, AbortSignal.any([context.signal, shutdown.signal]),
        configuration.onHostWork ? (notice, active) => { configuration.accounts.assertCurrent(context); return configuration.onHostWork!(context, notice, active); } : undefined);
      })();
      semanticClients.set(context, client);
      void client.then(() => undefined, () => { if (semanticClients.get(context) === client) semanticClients.delete(context); });
      }
      const resolved = await client; configuration.accounts.assertCurrent(context); return resolved;
    },
  });
}
