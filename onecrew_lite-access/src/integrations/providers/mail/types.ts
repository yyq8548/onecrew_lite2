export type MailOAuthProvider = "google" | "outlook";

export type MailProviderCapabilities = {
  can_send: boolean;
  can_read_thread: boolean;
  can_read_attachment: false;
};

export type MailOAuthConfig = {
  provider: MailOAuthProvider;
  encryptionKey: string;
};

export type MailOAuthIdentity = {
  subject: string;
  email: string;
  scopes: string[];
};

export type MailOAuthTokens = MailOAuthIdentity & {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};

export type MailOAuthRefresh = {
  access_token: string;
  refresh_token?: string;
  expires_in: number;
  scopes?: string[];
};

export interface MailOAuthAdapter {
  readonly provider: MailOAuthProvider;
  readonly supported: boolean;
  readonly callbackPath?: string;
  createConfig(bindings: unknown, authOrigin: string): MailOAuthConfig | null;
  authorizationUrl(
    config: MailOAuthConfig,
    state: string,
    nonce: string,
    codeVerifier: string,
  ): Promise<URL>;
  callbackUrl(config: MailOAuthConfig, parameters: URLSearchParams): URL;
  exchangeCode(
    config: MailOAuthConfig,
    callbackUrl: URL,
    state: string,
    nonce: string,
    codeVerifier: string,
  ): Promise<MailOAuthTokens>;
  refresh(config: MailOAuthConfig, refreshToken: string): Promise<MailOAuthRefresh>;
  revoke(config: MailOAuthConfig, refreshToken: string): Promise<void>;
  isReauthRequired(error: unknown): boolean;
  capabilities(scopes: string[]): MailProviderCapabilities;
}

export type MailOAuthRegistry = Record<MailOAuthProvider, MailOAuthAdapter>;
