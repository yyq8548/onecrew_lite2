import { infiniteQueryOptions, keepPreviousData } from "@tanstack/react-query";

import {
  creatorInvoke,
  type BulkMoveCreatorsToGroupInput,
  type CreateCreatorGroupInput,
  type CreatorGroup,
  type CreatorGroupListResult,
  type DeleteCreatorGroupInput,
  type DeleteCreatorGroupResult,
  type ListCreatorGroupsInput,
  type MoveCreatorsToGroupResult,
  type SetCreatorGroupInput,
  type UpdateCreatorGroupInput,
} from "@/lib/creator";

export const CREATOR_GROUPS_QUERY_KEY = ["creator-creator-groups"] as const;

const GROUP_PAGE_LIMIT = 100;
const LOCAL_RUNTIME_STALE_TIME = 30_000;

export function shouldAutoFetchCreatorGroupPage(input: {
  selectedCount: number;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  hasError: boolean;
}): boolean {
  return input.selectedCount > 0
    && input.hasNextPage
    && !input.isFetchingNextPage
    && !input.hasError;
}

export function creatorGroupsInfiniteQueryOptions(
  enabled: boolean,
  criteria: { query?: string | null; include_archived?: boolean } = {},
) {
  const query = criteria.query?.trim() || null;
  const includeArchived = criteria.include_archived ?? false;
  return infiniteQueryOptions({
    queryKey: [
      ...CREATOR_GROUPS_QUERY_KEY,
      { query, include_archived: includeArchived },
    ] as const,
    queryFn: ({ pageParam }) => creatorInvoke<CreatorGroupListResult>(
      "onecrew_creator_group_list",
      {
        input: {
          limit: GROUP_PAGE_LIMIT,
          cursor: pageParam,
          query,
          include_archived: includeArchived,
        } satisfies ListCreatorGroupsInput,
      },
    ),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? null,
    placeholderData: keepPreviousData,
    enabled,
    retry: false,
    staleTime: LOCAL_RUNTIME_STALE_TIME,
    refetchOnWindowFocus: false,
  });
}

export async function createCreatorGroup(
  input: CreateCreatorGroupInput,
): Promise<CreatorGroup> {
  return creatorInvoke<CreatorGroup>("onecrew_creator_group_create", { input });
}

export async function updateCreatorGroup(
  input: UpdateCreatorGroupInput,
): Promise<CreatorGroup> {
  return creatorInvoke<CreatorGroup>("onecrew_creator_group_update", { input });
}

export async function deleteCreatorGroup(
  input: DeleteCreatorGroupInput,
): Promise<DeleteCreatorGroupResult> {
  return creatorInvoke<DeleteCreatorGroupResult>("onecrew_creator_group_delete", { input });
}

export async function setCreatorGroup(
  input: SetCreatorGroupInput,
): Promise<MoveCreatorsToGroupResult> {
  return creatorInvoke<MoveCreatorsToGroupResult>("onecrew_creator_group_set", { input });
}

export async function bulkMoveCreatorsToGroup(
  input: BulkMoveCreatorsToGroupInput,
): Promise<MoveCreatorsToGroupResult> {
  return creatorInvoke<MoveCreatorsToGroupResult>("onecrew_creator_group_bulk_move", { input });
}
