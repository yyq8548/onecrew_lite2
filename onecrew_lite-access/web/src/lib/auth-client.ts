import { createAuthClient } from "better-auth/react";

import { safeDistributionReturnTo } from "@/lib/distribution-return";
import { accessOrigin } from "@/lib/access-origin";

const socialAuthClient = createAuthClient(
  accessOrigin ? { baseURL: accessOrigin } : {},
);

export function startGoogleOAuth(returnTo?: string) {
  const safeReturnTo = safeDistributionReturnTo(returnTo);
  return socialAuthClient.signIn.social({
    provider: "google",
    ...(safeReturnTo ? { returnTo: safeReturnTo } : {}),
  });
}
