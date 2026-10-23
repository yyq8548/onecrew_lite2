import type { EmailFolder, EmailThreadFolder } from "@/lib/creator";

/** Archive is an Inbox action; Sent membership is derived from outbound mail. */
export function shouldShowArchiveAction(
  viewFolder: EmailFolder | undefined,
  threadFolder: EmailThreadFolder | undefined,
): boolean {
  return viewFolder === "inbox" && threadFolder === "inbox";
}

export function shouldShowUnarchiveAction(
  viewFolder: EmailFolder | undefined,
  threadFolder: EmailThreadFolder | undefined,
): boolean {
  return viewFolder === "archive" && threadFolder === "archive";
}

export function shouldShowRestoreAction(
  viewFolder: EmailFolder | undefined,
  threadFolder: EmailThreadFolder | undefined,
): boolean {
  return viewFolder === "deleted" && threadFolder === "deleted";
}
