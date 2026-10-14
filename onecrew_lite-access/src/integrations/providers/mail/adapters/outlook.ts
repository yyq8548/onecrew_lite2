import { IntegrationError } from "../../../../devices/errors";
import type { MailOAuthAdapter, MailOAuthConfig } from "../types";

export class OutlookMailOAuthAdapter implements MailOAuthAdapter {
  readonly provider = "outlook" as const;
  readonly supported = false;

  createConfig(_bindings: unknown, _authOrigin: string): MailOAuthConfig | null {
    return null;
  }

  private unsupported(): never {
    throw new IntegrationError("not_supported", 501);
  }

  authorizationUrl(_config: MailOAuthConfig, _state: string, _nonce: string, _verifier: string): Promise<URL> {
    return Promise.reject(this.unsupported());
  }

  callbackUrl(): URL {
    return this.unsupported();
  }

  exchangeCode(): never {
    return this.unsupported();
  }

  refresh(): never {
    return this.unsupported();
  }

  revoke(): never {
    return this.unsupported();
  }

  isReauthRequired(_error: unknown) {
    return false;
  }

  capabilities() {
    return { can_send: false, can_read_thread: false, can_read_attachment: false as const };
  }
}

export const outlookMailOAuthAdapter = new OutlookMailOAuthAdapter();
