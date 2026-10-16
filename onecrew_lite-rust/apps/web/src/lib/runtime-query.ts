import { invoke } from "@/lib/local-runtime";
import { infiniteQueryOptions, keepPreviousData, queryOptions } from "@tanstack/react-query";

import { appI18n } from "@/i18n";
import zhCNCommon from "@/i18n/locales/zh-CN/common";
import {
  CREATORS_QUERY_KEY,
  creatorInvoke,
  type CreatorRecord,
  type CreatorListPage,
  type ListCreatorsInput,
} from "@/lib/creator";
import { normalizeCreatorListCriteria, type CreatorListCriteria } from "@/lib/creator-list-query";
import type { ServiceCredential } from "@/types";

export const YOUTUBE_CREDENTIALS_QUERY_KEY = ["service-credentials", "youtube"] as const;

const LOCAL_RUNTIME_STALE_TIME = 30_000;

/** Canonical backend page maximum; virtual lists follow the returned cursor. */
const LIST_PAGE_LIMIT = 100;

/** Creator list family root: invalidate after mutations that only change list rows. */
export const CREATOR_LIST_ROOT_QUERY_KEY = [...CREATORS_QUERY_KEY, "list"] as const;

export const creatorDetailQueryKey = (creatorId: string | null) => (
  [...CREATORS_QUERY_KEY, "detail", creatorId] as const
);

export function creatorsQueryOptions(
  enabled: boolean,
  criteria: CreatorListCriteria = {},
) {
  const normalized = normalizeCreatorListCriteria(criteria);
  const input: ListCreatorsInput = {
    ...normalized,
    limit: LIST_PAGE_LIMIT,
    cursor: null,
    include_facets: true,
  };
  return queryOptions({
    // List family key: root + "list" + normalized criteria (one root per family).
    queryKey: [
      ...CREATOR_LIST_ROOT_QUERY_KEY,
      normalized,
      "page",
    ] as const,
    // Bounded callers explicitly consume one page; keep their cache shape
    // separate from the cursor-following list below.
    queryFn: () => creatorInvoke<CreatorListPage>("onecrew_creator_list", { input }),
    enabled,
    retry: false,
    staleTime: LOCAL_RUNTIME_STALE_TIME,
    refetchOnWindowFocus: false,
  });
}

/** Cursor-following creator list: the backend query matches display name,
 * latest linked-account handles, and saved contact-route types or values; each
 * page continues the same normalized criteria. */
export function creatorsInfiniteQueryOptions(
  enabled: boolean,
  criteria: CreatorListCriteria = {},
) {
  const normalized = normalizeCreatorListCriteria(criteria);
  return infiniteQueryOptions({
    queryKey: [
      ...CREATOR_LIST_ROOT_QUERY_KEY,
      normalized,
    ] as const,
    queryFn: ({ pageParam }) => creatorInvoke<CreatorListPage>("onecrew_creator_list", {
      input: {
        ...normalized,
        limit: LIST_PAGE_LIMIT,
        cursor: pageParam,
        include_facets: pageParam === null,
      } satisfies ListCreatorsInput,
    }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    // Keep the previous page visible while a changed search term fetches.
    placeholderData: keepPreviousData,
    enabled,
    retry: false,
    staleTime: LOCAL_RUNTIME_STALE_TIME,
    refetchOnWindowFocus: false,
  });
}

export function creatorDetailQueryOptions(creatorId: string | null, enabled: boolean) {
  return queryOptions({
    queryKey: creatorDetailQueryKey(creatorId),
    queryFn: () => {
      if (!creatorId) {
        throw new Error(appI18n.isInitialized
          ? appI18n.t("errors.creatorIdRequired", { ns: "common" })
          : zhCNCommon.errors.creatorIdRequired);
      }
      return creatorInvoke<CreatorRecord>("onecrew_creator_get", { input: { creator_id: creatorId } });
    },
    enabled: enabled && creatorId !== null,
    retry: false,
    staleTime: LOCAL_RUNTIME_STALE_TIME,
    refetchOnWindowFocus: false,
  });
}

export function youtubeCredentialsQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: YOUTUBE_CREDENTIALS_QUERY_KEY,
    queryFn: () => invoke<ServiceCredential[]>("list_youtube_api_keys"),
    enabled,
    retry: false,
    staleTime: LOCAL_RUNTIME_STALE_TIME,
    refetchOnWindowFocus: false,
  });
}
