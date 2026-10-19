import { lstat } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { VerifiedMailIdentityProvider } from "../mail/gmail-access.ts";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { AccountBrowserBindingResolver, type BrowserAccountDirectoryResolver, type TrustedExtensionContextProofVerifier } from "../platforms/browser/binding.ts";
import type { PlatformVaultReader } from "../platforms/browser/platform-vault.ts";
import { OpenCliAddons } from "../platforms/opencli/addons.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { OpenCliReader } from "../platforms/opencli/reader.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { resolveCodexCli } from "./codex-installation.ts";
import { CodexSemanticClient, type SemanticStructuredClient } from "./semantic-client.ts";
import type { DiscoveryBrowserResolver } from "./search-execution.ts";

export interface DiscoveryReadEnvironment { readonly reader: OpenCliReader; readonly browsers: DiscoveryBrowserResolver }
export interface DiscoveryDependencyResolver { reads(context: AccountContext): Promise<DiscoveryReadEnvironment>; semantic(context: AccountContext): Promise<SemanticStructuredClient> }
export interface TrustedDiscoveryConfiguration {
  openCliHome?(context: AccountContext): Promise<string>;
  accounts: AccountContextService;
  identities: VerifiedMailIdentityProvider;
  directories: BrowserAccountDirectoryResolver;
  browsers?: DiscoveryBrowserResolver;
  vault?: PlatformVaultReader;
  extensionProofs?: TrustedExtensionContextProofVerifier;
  codexExecutable?: string;
}
let semanticDirectories: Promise<void> = Promise.resolve();
function prepareSemanticDirectory(path: string, assertOwned: () => void): Promise<void> {
  const work = semanticDirectories.then(async () => {
    assertOwned();
    const existing = await lstat(path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (existing === null) await createPrivateDirectory(path); else await verifyPrivateDirectory(path);
    assertOwned();
  });
  semanticDirectories = work.then(() => undefined, () => undefined); return work;
}
// 创建工厂不探测CLI、读取凭据、安装扩展或打开浏览器。配置只由受信主程序提供。
export function createDefaultDiscoveryDependencies(configuration: TrustedDiscoveryConfiguration): DiscoveryDependencyResolver {
  return Object.freeze({
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
      const owned = await configuration.directories.accountDirectory(context); owned.assertOwned();
      if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_semantic_directory_owner_mismatch");
      const executable = await resolveCodexCli(configuration.codexExecutable); configuration.accounts.assertCurrent(context); owned.assertOwned();
      const working = join(owned.directory, "discovery-semantic");
      await prepareSemanticDirectory(working, () => { configuration.accounts.assertCurrent(context); owned.assertOwned(); });
      configuration.accounts.assertCurrent(context); owned.assertOwned(); return new CodexSemanticClient(executable, working);
    },
  });
}
