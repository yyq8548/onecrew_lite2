import { invoke } from "@/lib/local-runtime";
import { queryOptions, type QueryClient } from "@tanstack/react-query";

import { CONNECTED_ACCOUNTS_QUERY_KEY } from "@/lib/creator";
import type { Account } from "@/types";

export function connectedProfilesQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: CONNECTED_ACCOUNTS_QUERY_KEY,
    queryFn: () => invoke<Account[]>("list_profiles"),
    enabled,
    retry: false,
    staleTime: 30_000,
    refetchOnWindowFocus: false,
  });
}

export function invalidateConnectedProfiles(queryClient: QueryClient) {
  return queryClient.invalidateQueries({ queryKey: CONNECTED_ACCOUNTS_QUERY_KEY });
}
