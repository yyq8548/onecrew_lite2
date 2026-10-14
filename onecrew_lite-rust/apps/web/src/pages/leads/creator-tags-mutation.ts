import type { QueryClient } from "@tanstack/react-query";
import { CREATORS_QUERY_KEY, updateCreator } from "@/lib/creator";
import { creatorDetailQueryKey } from "@/lib/runtime-query";

export async function saveCreatorTags(queryClient: QueryClient, creatorId: string, tags: string[]) {
  // The canonical update returns the complete persisted record. Populate that
  // readback immediately, then refresh every list/detail that can show tags.
  const record = await updateCreator({ creator_id: creatorId, tags });
  queryClient.setQueryData(creatorDetailQueryKey(creatorId), record);
  await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
  return record;
}

export function creatorWriteErrorDetail(cause: unknown): string | null {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === "string") return cause;
  if (cause && typeof cause === "object" && "message" in cause && typeof cause.message === "string") {
    return cause.message;
  }
  return null;
}
