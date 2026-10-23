import { z } from "zod";
const text = z.string().refine(value => value.isWellFormed());
const nullableText = text.nullish().transform(value => value ?? null);
export const searchPolicyFields = { search_plan_policy: z.enum(["require_confirmation", "user_requested_direct"]).default("require_confirmation"), confirmed_plan_ref: nullableText };
const count = (max: number) => z.number().int().min(1).max(max).nullish().transform(value => value ?? 20);
const query = text.refine(value => !!value.trim());
const publishedAfter = text.refine(value => /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/u.test(value) && z.iso.datetime({ precision: 0 }).safeParse(value.replace(/:60Z$/u, ":59Z")).success).nullish().transform(value => value ?? null);
export const youtubeSearchInput = z.strictObject({ ...searchPolicyFields, query, count: count(50), order: z.enum(["relevance", "date"]).default("relevance"), published_after: publishedAfter, cursor: nullableText, region_code: nullableText, language: nullableText });
export const instagramSearchInput = z.strictObject({ ...searchPolicyFields, query, count: count(100) });
export const telegramSearchInput = z.strictObject({ ...searchPolicyFields, query, count: count(100) });
const list = z.array(text).max(20).default([]);
const integer = z.number().int().min(0).max(4294967295).nullish().transform(value => value ?? null);
export const advancedXSearchInput = z.strictObject({ ...searchPolicyFields,
  all_words: list, exact_phrase: nullableText, any_words: list, exclude_words: list, hashtags: list,
  from_accounts: list, to_accounts: list, mentioning_accounts: list, language: nullableText, since: nullableText, until: nullableText,
  min_replies: integer, min_likes: integer, min_reposts: integer, replies: z.enum(["any", "include", "exclude"]).default("any"), links: z.enum(["any", "include", "exclude"]).default("any"), result_mode: z.enum(["top", "latest"]).default("top"), count: count(100), cursor: nullableText,
});
export type AdvancedXSearchInput = z.infer<typeof advancedXSearchInput>;
export function compileAdvancedXQuery(input: AdvancedXSearchInput): string {
  const word = (value: string) => /^[\p{L}\p{M}\p{N}_]+(?:[-'’][\p{L}\p{M}\p{N}_]+)*$/u.test(value) && /[\p{L}\p{N}_]/u.test(value);
  const atoms = (values: readonly string[]) => values.map(value => {
    const phrase = value.includes(" ");
    if (value.trim() !== value || [...value].length > 64 || (phrase ? !value.split(" ").every(word) : !word(value) || ["or", "and"].includes(value.toLowerCase()))) throw new Error("advanced_search_atom_invalid");
    return phrase ? `"${value}"` : value;
  });
  const prefixed = (values: readonly string[], prefix: string, valid: (value: string) => boolean) => values.map(value => {
    const normalized = value.startsWith(prefix) ? value.slice(1) : value;
    if (value.trim() !== value || !valid(normalized)) throw new Error("advanced_search_selector_invalid"); return normalized;
  });
  const handles = (values: readonly string[]) => prefixed(values, "@", value => /^[A-Za-z\d_]{1,15}$/u.test(value));
  const terms: string[] = [], all = atoms(input.all_words), any = atoms(input.any_words), excluded = atoms(input.exclude_words);
  terms.push(...all);
  if (input.exact_phrase !== null) { const value = input.exact_phrase; if (!value.trim() || value.trim() !== value || [...value].length > 256 || /["\\\p{Cc}]/u.test(value)) throw new Error("advanced_search_phrase_invalid"); terms.push(`"${value}"`); }
  const group = (values: string[]) => { if (values.length) terms.push(values.length === 1 ? values[0]! : `(${values.join(" OR ")})`); };
  group(any); terms.push(...excluded.map(value => `-${value}`));
  const tags = prefixed(input.hashtags, "#", value => /^[\p{L}\p{M}\p{N}_]+$/u.test(value) && /[\p{L}\p{N}_]/u.test(value));
  const from = handles(input.from_accounts), to = handles(input.to_accounts), mentions = handles(input.mentioning_accounts);
  group(tags.map(value => `#${value}`)); group(from.map(value => `from:${value}`)); group(to.map(value => `to:${value}`)); group(mentions.map(value => `@${value}`));
  if (!all.length && input.exact_phrase === null && !any.length && !tags.length && !from.length && !to.length && !mentions.length) throw new Error("advanced_search_positive_selector_required");
  if (input.language !== null) { if (!/^[a-z]{2,3}(?:-[a-z\d]{2,8})?$/u.test(input.language)) throw new Error("advanced_search_language_invalid"); terms.push(`lang:${input.language}`); }
  for (const name of ["replies", "links"] as const) if (input[name] !== "any") terms.push(`${input[name] === "exclude" ? "-" : ""}filter:${name}`);
  for (const [field, operator] of [["min_replies", "min_replies"], ["min_likes", "min_faves"], ["min_reposts", "min_retweets"]] as const) if (input[field] !== null) terms.push(`${operator}:${input[field]}`);
  for (const field of ["since", "until"] as const) if (input[field] !== null) { const value = input[field]; if (!z.iso.date().safeParse(value).success || value.slice(0, 4) < "2006") throw new Error("advanced_search_date_invalid"); terms.push(`${field}:${value}`); }
  if (input.since !== null && input.until !== null && input.since > input.until) throw new Error("advanced_search_date_range_reversed");
  const compiled = terms.join(" "); if ([...compiled].length > 512) throw new Error("advanced_search_query_too_long"); return compiled;
}
