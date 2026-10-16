import type { QueryClient, QueryKey } from "@tanstack/react-query";

import {
  EMAIL_TEMPLATE_LIST_QUERY_KEY,
  MAILBOX_QUERY_KEY,
  emailDraftKey,
  emailDraftListRootKey,
  emailFolderListKey,
  emailThreadKey,
  emailThreadListRootKey,
  mailboxSyncStatusKey,
} from "@/components/mail/queries";
import {
  CONNECTED_ACCOUNTS_QUERY_KEY,
  CREATORS_QUERY_KEY,
  INBOX_ATTENTION_QUERY_KEY,
  REPLY_TRIAGE_QUERY_KEY,
} from "@/lib/creator";
import { CONVERSATION_LIST_QUERY_KEY } from "@/lib/conversations";
import { CREATOR_GROUPS_QUERY_KEY } from "@/lib/creator-groups";
import {
  YOUTUBE_CREDENTIALS_QUERY_KEY,
} from "@/lib/runtime-query";

export const RUNTIME_CHANGE_EVENT = "runtime://changed";

export type RuntimeChangeScope =
  | "all"
  | "operator_accounts"
  | "service_credentials"
  | "email_folders"
  | "email_threads"
  | "email_drafts"
  | "email_templates"
  | "email_sync"
  | "outreach"
  | "conversations"
  | "creators"
  | "creator_groups"
  | "reply_triage";

export type RuntimeChange = {
  // Positive values order one service merge; zero is a forced-refetch sentinel.
  // Neither is a durable cursor, and invalidation never depends on ordering.
  sequence: number;
  scopes: RuntimeChangeScope[];
};

const EMAIL_FOLDER_LIST_ROOT_QUERY_KEY = emailFolderListKey(null).slice(0, 1);
const EMAIL_THREAD_LIST_ROOT_QUERY_KEY = emailThreadListRootKey(null).slice(0, 1);
const EMAIL_THREAD_ROOT_QUERY_KEY = emailThreadKey(null).slice(0, 1);
const EMAIL_DRAFT_LIST_ROOT_QUERY_KEY = emailDraftListRootKey(null).slice(0, 1);
const EMAIL_DRAFT_ROOT_QUERY_KEY = emailDraftKey(null).slice(0, 1);
const EMAIL_SYNC_STATUS_ROOT_QUERY_KEY = mailboxSyncStatusKey(null).slice(0, 1);

const QUERY_KEYS_BY_SCOPE: Record<Exclude<RuntimeChangeScope, "all">, readonly QueryKey[]> = {
  operator_accounts: [CONNECTED_ACCOUNTS_QUERY_KEY],
  service_credentials: [
    YOUTUBE_CREDENTIALS_QUERY_KEY,
    MAILBOX_QUERY_KEY,
  ],
  email_folders: [EMAIL_FOLDER_LIST_ROOT_QUERY_KEY, MAILBOX_QUERY_KEY],
  email_threads: [
    EMAIL_FOLDER_LIST_ROOT_QUERY_KEY,
    EMAIL_THREAD_LIST_ROOT_QUERY_KEY,
    EMAIL_THREAD_ROOT_QUERY_KEY,
    INBOX_ATTENTION_QUERY_KEY,
  ],
  email_drafts: [
    EMAIL_FOLDER_LIST_ROOT_QUERY_KEY,
    EMAIL_DRAFT_LIST_ROOT_QUERY_KEY,
    EMAIL_DRAFT_ROOT_QUERY_KEY,
  ],
  email_templates: [EMAIL_TEMPLATE_LIST_QUERY_KEY],
  email_sync: [EMAIL_SYNC_STATUS_ROOT_QUERY_KEY],
  outreach: [
    ["creator-outreach-target"],
    ["creator-message-preparation"],
    ["creator-contact-route-last-contact-account"],
  ],
  // 会话域比 outreach 窄:只有本地线程读与收件箱列表。同步只发这个
  // scope,所以失效永远落在本地读上,不会再触发一次同步(那会成环)。
  // 收件箱列表的 key 不是 ["creator-conversation"] 的前缀,必须单列。
  conversations: [
    ["creator-conversation"],
    CONVERSATION_LIST_QUERY_KEY,
    INBOX_ATTENTION_QUERY_KEY,
  ],
  creators: [CREATORS_QUERY_KEY, CREATOR_GROUPS_QUERY_KEY],
  creator_groups: [CREATOR_GROUPS_QUERY_KEY],
  // 一条新裁决要刷新的正是「显示它的那些行」:两个 inbox 列表和
  // 裁决列表本身。刻意不含任何会回头找 provider 的 key——会话同步
  // 成环那次的教训就在这里。
  reply_triage: [
    REPLY_TRIAGE_QUERY_KEY,
    CONVERSATION_LIST_QUERY_KEY,
    EMAIL_THREAD_LIST_ROOT_QUERY_KEY,
  ],
};

const ALL_RUNTIME_QUERY_KEYS = Object.values(QUERY_KEYS_BY_SCOPE).flat();

export function queryKeysForRuntimeChangeScopes(
  scopes: readonly RuntimeChangeScope[],
): QueryKey[] {
  const selected = scopes.includes("all")
    ? ALL_RUNTIME_QUERY_KEYS
    : scopes.flatMap((scope) => scope === "all" ? [] : QUERY_KEYS_BY_SCOPE[scope]);
  const unique = new Map<string, QueryKey>();
  for (const queryKey of selected) unique.set(JSON.stringify(queryKey), queryKey);
  return [...unique.values()];
}

export async function invalidateRuntimeChangeScopes(
  queryClient: QueryClient,
  scopes: readonly RuntimeChangeScope[],
): Promise<void> {
  await Promise.all(
    queryKeysForRuntimeChangeScopes(scopes).map((queryKey) => (
      queryClient.invalidateQueries({ queryKey })
    )),
  );
}
