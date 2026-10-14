import { safeAuthReturnTo } from "../../../src/auth/return-to";
export const safeDistributionReturnTo = safeAuthReturnTo;

export function distributionReturnFromSearch(search: string) {
  return safeDistributionReturnTo(new URLSearchParams(search).get("returnTo"));
}
