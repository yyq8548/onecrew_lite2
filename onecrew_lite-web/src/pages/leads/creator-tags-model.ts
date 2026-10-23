export type CreatorTagsDraft = { tags: string[]; text: string };

export type CreatorTagsValidationKey =
  | "tags.validation.tooMany"
  | "tags.validation.length"
  | "tags.validation.characters"
  | "tags.validation.duplicate";

export type CreatorTagsResult =
  | { tags: string[]; error: null }
  | { tags: null; error: CreatorTagsValidationKey };

/** Rust str::trim uses Unicode White_Space, rather than JS trim's BOM rule. */
export function trimCreatorTag(value: string) {
  return value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
}

/** Form feedback mirrors the canonical runtime constraints. The runtime still
 * validates every HTTP/MCP write; this helper never truncates user input. */
export function validateCreatorTags(values: readonly string[]): CreatorTagsResult {
  if (values.length > 3) return { tags: null, error: "tags.validation.tooMany" };
  const tags: string[] = [];
  for (const value of values) {
    // Check before trimming, as the owner does: boundary newlines/NEL are
    // invalid too. Unpaired UTF-16 surrogates cannot encode a Unicode scalar.
    if (/[\p{Cc}\p{Cs}]/u.test(value)) return { tags: null, error: "tags.validation.characters" };
    const tag = trimCreatorTag(value);
    if (!tag || Array.from(tag).length > 12) return { tags: null, error: "tags.validation.length" };
    if (tags.includes(tag)) return { tags: null, error: "tags.validation.duplicate" };
    tags.push(tag);
  }
  return { tags, error: null };
}

/** Include an unfinished input in explicit Save/Create, so clicking the form
 * action cannot silently discard a label the user has just typed. */
export function creatorTagsFromDraft(draft: CreatorTagsDraft): CreatorTagsResult {
  return validateCreatorTags(draft.text === "" ? draft.tags : [...draft.tags, draft.text]);
}
