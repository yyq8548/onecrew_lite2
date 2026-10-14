declare global {
  interface CloudflareBindings {
    CLI_RELEASE_VERSION: string;
    RELEASE_LINK_SECRET?: string;
    RELEASE_PUBLISHER_TOKEN?: string;
    GMAIL_CLIENT_ID?: string;
    GMAIL_CLIENT_SECRET?: string;
    THIRD_PARTY_TOKEN_ENCRYPTION_KEY?: string;
    ACCESS_ORIGIN: string;
    BETTER_AUTH_SECRET: string;
    EDGE_RATE_LIMITER: RateLimit;
    GOOGLE_CLIENT_ID?: string;
    GOOGLE_CLIENT_SECRET?: string;
    PAIRING_HMAC_SECRET: string;
  }
}

export type Bindings = CloudflareBindings;

export type AppEnv = {
  Bindings: Bindings;
  Variables: {
    auth: import("../auth/auth").ReacherAuth;
    config: import("../config/env").RuntimeConfig;
    webOrigin: string;
    mailOwner: import("../devices/credential-identity").DeviceOwner;
    mailRequestId: string;
  };
};
