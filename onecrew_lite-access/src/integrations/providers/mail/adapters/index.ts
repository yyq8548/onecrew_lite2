import { IntegrationError } from "../../../../devices/errors";
import { googleMailOAuthAdapter } from "./google";
import { outlookMailOAuthAdapter } from "./outlook";
import type { MailOAuthAdapter, MailOAuthConfig, MailOAuthProvider } from "../types";

export type MailOAuthRegistry = Record<MailOAuthProvider, MailOAuthAdapter>;

export function createMailOAuthRegistry(
  adapters: Partial<MailOAuthRegistry> = {},
): MailOAuthRegistry {
  return {
    google: adapters.google ?? googleMailOAuthAdapter,
    outlook: adapters.outlook ?? outlookMailOAuthAdapter,
  };
}

export function getMailOAuthAdapter(provider: MailOAuthProvider, registry: MailOAuthRegistry = createMailOAuthRegistry()) {
  const adapter = registry[provider];
  if (!adapter.supported) throw new IntegrationError("not_supported", 501);
  return adapter;
}

export function mailOAuthConfig(
  provider: MailOAuthProvider,
  bindings: unknown,
  authOrigin: string,
  registry: MailOAuthRegistry = createMailOAuthRegistry(),
): { adapter: MailOAuthAdapter; config: MailOAuthConfig } {
  const adapter = getMailOAuthAdapter(provider, registry);
  const config = adapter.createConfig(bindings, authOrigin);
  if (!config) throw new IntegrationError("configuration_required", 503);
  return { adapter, config };
}
