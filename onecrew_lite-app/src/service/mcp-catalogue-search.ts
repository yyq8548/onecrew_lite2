import MiniSearch from "minisearch";
import { newStemmer } from "snowball-stemmers";
import { mcpToolSearchInput } from "./mcp-contracts.ts";
import type { McpSurface } from "./mcp-sessions.ts";
import { McpCatalogue, type McpCatalogueData, type McpOperationEntry, type McpPlaybookEntry } from "./mcp-catalogue.ts";

// 原 Tantivy 0.26.1 English StopWordFilter 的 Lucene 列表。
const stopWords = new Set(["a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these", "they", "this", "to", "was", "will", "with"]);
const stemmer = newStemmer("english");
const tokenize = (text: string) => text.split(/[^\p{Alphabetic}\p{Number}]+/u).filter(Boolean);
function processTerm(term: string): string | null { if (Buffer.byteLength(term) >= 40) return null; const lower = term.toLowerCase(); return stopWords.has(lower) ? null : stemmer.stem(lower); }
export function mcpEnglishSearchTerms(text: string): string[] { return [...new Set(tokenize(text).map(processTerm).filter((term): term is string => term !== null))].sort(); }
const compareId = (left: string, right: string) => Buffer.compare(Buffer.from(left), Buffer.from(right));
type SearchInput = ReturnType<typeof mcpToolSearchInput.parse>;
type Document = { id: string; title: string; summary: string; aliases: string; use_case: string; linked_titles: string; steps: string; pitfalls: string };
const fields = ["title", "summary", "aliases", "use_case", "linked_titles", "steps", "pitfalls"];
function index(documents: Document[]) { const result = new MiniSearch<Document>({ fields, tokenize, processTerm, searchOptions: { prefix: false, fuzzy: false, combineWith: "OR", bm25: { k: 1.2, b: 0.75, d: 0 } } }); result.addAll(documents); return result; }
export class McpCatalogueSearch {
  readonly #catalogue: McpCatalogue;
  readonly #operations: MiniSearch<Document>;
  readonly #playbooks: MiniSearch<Document>;
  constructor(catalogue: McpCatalogue, data: McpCatalogueData) {
    this.#catalogue = catalogue;
    const operations = data.entries.filter((entry): entry is McpOperationEntry => entry.kind === "operation");
    const base = (value: McpOperationEntry | McpPlaybookEntry): Document => ({ id: value.metadata.id, title: value.metadata.title, summary: value.metadata.summary, aliases: value.metadata.aliases.join(" "), use_case: "", linked_titles: "", steps: "", pitfalls: "" });
    this.#operations = index(operations.map(base));
    this.#playbooks = index(data.entries.filter((entry): entry is McpPlaybookEntry => entry.kind === "playbook").map(value => ({ ...base(value), use_case: value.use_case, linked_titles: [...value.primary_operation_ids, ...value.supporting_operation_ids].flatMap(id => operations.filter(operation => operation.metadata.id === id).map(operation => operation.metadata.title)).join(" "), steps: value.recommended_steps.map(step => step.instruction).join(" "), pitfalls: value.known_pitfalls.join(" ") })));
  }
  #recall(index: MiniSearch<Document>, query: string, allowed: ReadonlySet<string>, playbooks: boolean) {
    const count = mcpEnglishSearchTerms(query).length, minimum = playbooks ? count <= 4 ? count : Math.ceil(count * 3 / 4) : Math.max(1, Math.ceil(count * 2 / 5));
    const core = playbooks ? { use_case: 9, title: 8, aliases: 4, summary: 4 } : { title: 8, aliases: 4, summary: 3 };
    // MiniSearch 在 BM25 总和上额外乘命中词数；原 Tantivy BooleanQuery 仅相加。
    const hits = index.search(query, { fields: Object.keys(core), boost: core, filter: hit => typeof hit.id === "string" && allowed.has(hit.id) }).filter(hit => new Set(hit.queryTerms).size >= minimum).map(hit => ({ id: String(hit.id), score: hit.score / (hit.queryTerms.length || 1) }));
    if (playbooks) {
      const bonus = new Map(index.search(query, { fields: ["linked_titles", "steps", "pitfalls"], boost: { linked_titles: 3, steps: 1, pitfalls: 0.5 }, filter: hit => typeof hit.id === "string" && allowed.has(hit.id) }).map(hit => [String(hit.id), hit.score / (hit.queryTerms.length || 1)]));
      for (const hit of hits) hit.score += bonus.get(hit.id) ?? 0;
    }
    return hits.sort((a, b) => b.score - a.score || compareId(a.id, b.id));
  }
  search(surface: McpSurface, raw: unknown) {
    const input = mcpToolSearchInput.parse(raw), query = input.query.trim();
    if ([...query].some(character => character.codePointAt(0)! > 127 && /[\p{Alphabetic}\p{Number}]/u.test(character))) throw new Error("search query must be normalized English; translate non-English text first");
    if (!mcpEnglishSearchTerms(query).length) throw new Error("search query must contain at least one meaningful English term");
    const allowed = this.#catalogue.operations(surface).filter(operation => this.#allowed(operation, input)), ids = new Set(allowed.map(operation => operation.metadata.id));
    const playbooks = this.#catalogue.playbooks(surface).filter(playbook => (!input.toolkits.length || playbook.toolkit_ids.every(toolkit => input.toolkits.includes(toolkit))) && (!input.effects.length || [...playbook.primary_operation_ids, ...playbook.supporting_operation_ids].every(id => allowed.some(operation => operation.metadata.id === id))));
    let selected: McpPlaybookEntry[] = [], operations: McpOperationEntry[] = [];
    const dotted = query.includes(".") && /^[A-Za-z0-9._-]+$/u.test(query), legacy = query.includes("_") && !query.includes(".") && /^[A-Za-z0-9_-]+$/u.test(query);
    if (dotted) {
      const exact = allowed.find(operation => operation.metadata.id === query), playbook = playbooks.find(playbook => playbook.metadata.id === query);
      if (exact) operations = [exact];
      else if (playbook) { selected = [playbook]; operations = [...playbook.primary_operation_ids, ...playbook.supporting_operation_ids].flatMap(id => allowed.filter(operation => operation.metadata.id === id)).slice(0, input.limit); }
    } else if (!legacy) {
      selected = this.#recall(this.#playbooks, query, new Set(playbooks.map(playbook => playbook.metadata.id)), true).flatMap(hit => playbooks.filter(playbook => playbook.metadata.id === hit.id)).slice(0, 3);
      const primary = new Set(selected[0]?.primary_operation_ids), supporting = new Set(selected[0]?.supporting_operation_ids), scores = new Map(this.#recall(this.#operations, query, ids, false).map(hit => [hit.id, hit.score]));
      for (const id of primary) if (ids.has(id)) scores.set(id, (scores.get(id) ?? 0) + 8);
      for (const id of supporting) if (ids.has(id)) scores.set(id, (scores.get(id) ?? 0) + 4);
      const rank = (id: string) => !selected.length || primary.has(id) ? 0 : supporting.has(id) ? 1 : 2;
      operations = [...scores.entries()].sort(([a, av], [b, bv]) => rank(a) - rank(b) || bv - av || compareId(a, b)).slice(0, input.limit).flatMap(([id]) => allowed.filter(operation => operation.metadata.id === id));
    }
    const terms = new Set(query.toLowerCase().split(/[^a-z0-9]+/u)), guides = new Set(input.toolkits);
    for (const [toolkit, aliases] of [["x", ["x", "twitter"]], ["instagram", ["instagram"]], ["telegram", ["telegram"]], ["youtube", ["youtube"]], ["onecrew", ["onecrew"]], ["email", ["email", "mail", "resend"]]] as const) if (aliases.some(alias => terms.has(alias))) guides.add(toolkit);
    for (const operation of operations) guides.add(operation.metadata.toolkit);
    for (const playbook of selected) for (const toolkit of playbook.toolkit_ids) guides.add(toolkit);
    return { operations: operations.map(operation => ({ id: operation.metadata.id, toolkit: operation.metadata.toolkit, title: operation.metadata.title, summary: operation.metadata.summary, effects: [operation.effect], connection: operation.connection })), toolkit_guides: this.#catalogue.directory(surface).filter(guide => guides.has(guide.toolkit)).map(({ id: _id, ...guide }) => guide).slice(0, 8), workflow_hints: selected.map(playbook => ({ id: playbook.metadata.id, title: playbook.metadata.title, summary: playbook.metadata.summary, operation_ids: [...new Set([...playbook.primary_operation_ids, ...playbook.supporting_operation_ids])].filter(id => this.#catalogue.operation(id, surface)).slice(0, 8), steps: playbook.recommended_steps.map(step => step.instruction).slice(0, 8), pitfalls: playbook.known_pitfalls.slice(0, 8) })).filter(hint => hint.operation_ids.length && hint.steps.length).slice(0, 3) };
  }
  #allowed(operation: McpOperationEntry, input: SearchInput) { return (!input.toolkits.length || input.toolkits.includes(operation.metadata.toolkit)) && (!input.effects.length || input.effects.includes(operation.effect)); }
}
