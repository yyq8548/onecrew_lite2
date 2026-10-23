import { z } from "zod";
import { discoveryQualityScore, discoveryQualityMinimum as qualityMinimum, discoveryQualityFilter as qualityFilter, type DiscoveryQualityLevel, type DiscoveryQualityFilter } from "@onecrew/runtime-contracts/discovery";
export { qualityMinimum, qualityFilter };

export const SELECTION_QUALITY_POLICY = "selection-quality-evaluation-v2";
export const SELECTION_QUALITY_REVIEWER = "onecrew-selection-quality-v2";
const text = z.string().min(1).max(1000);
type Minimum = z.infer<typeof qualityMinimum>;
export type QualityFilter = DiscoveryQualityFilter;
export type ProposedQualityFilter = Minimum | { kind: "condition"; description: string; excerpt: string } | { kind: "all" | "any"; items: ProposedQualityFilter[] };
export const proposedQualityFilter: z.ZodType<ProposedQualityFilter> = z.lazy(() => z.union([
  qualityMinimum, z.strictObject({ kind: z.literal("condition"), description: text, excerpt: text }),
  z.strictObject({ kind: z.enum(["all", "any"]), items: z.array(proposedQualityFilter).min(1).max(32) }),
]));
export function qualityFilterNodes<T extends QualityFilter | ProposedQualityFilter>(rule: T): Array<QualityFilter | ProposedQualityFilter> {
  const result: Array<QualityFilter | ProposedQualityFilter> = [];
  function visit(node: QualityFilter | ProposedQualityFilter, depth: number) {
    if (depth > 8 || result.length >= 64) throw new Error("quality_filter_too_complex");
    result.push(node); if (node.kind === "all" || node.kind === "any") node.items.forEach(item => visit(item, depth + 1));
  }
  visit(rule, 0); return result;
}
export function validateProposedQualityFilter(request: string, rule: ProposedQualityFilter): boolean {
  const nodes = qualityFilterNodes(rule);
  if (!nodes.some(node => node.kind === "minimum")) return false;
  return nodes.every(node => {
    if (node.kind === "all" || node.kind === "any") return true;
    if (!("excerpt" in node) || !request.includes(node.excerpt)) return false;
    if (node.kind !== "minimum") return true;
    const quote = node.excerpt.toLowerCase();
    if (!/评分|分数|评级|档位|优秀|良好|中等|较低|低档|\d+\s*分|score|rating|tier|grade|excellent|good|medium|low/u.test(quote)) return false;
    if (!/至少|最低|不低于|以上|超过|大于|高于|只要|只选|仅限|不要|排除|at least|minimum|above|over|greater|only|exclude|>=|>/u.test(quote)) return false;
    if (/最好|尽量|优先|prefer|ideally/u.test(quote) && !/必须|至少|最低|只要|只选|仅限|must|required|at least|minimum|only/u.test(quote)) return false;
    if (node.operator === "gt" && /至少|最低|不低于|以上|at least|minimum|>=/u.test(quote)) return false;
    if (node.operator === "gte" && /超过|大于|高于|greater than|more than|(?<![=])>(?!=)/u.test(quote) && !/不低于|至少|minimum|at least/u.test(quote)) return false;
    const digits = quote.match(/\d+/gu)?.map(Number) ?? [];
    const named = ([['优秀', 'excellent', 95], ['良好', 'good', 85], ['中等', 'medium', 70], ['较低', 'low', 40]] as const).some(([zh, en, score]) => (quote.includes(zh) || quote.includes(en)) && node.score === score);
    return digits.includes(node.score) || named || node.score === 70 && /不要低档|排除低档|exclude low/u.test(quote);
  });
}
export function freezeQualityFilter(rule: ProposedQualityFilter, requirements: Array<{ description: string; excerpt: string }>): QualityFilter {
  if (rule.kind === "minimum") return rule;
  if ("items" in rule) return { kind: rule.kind, items: rule.items.map(item => freezeQualityFilter(item, requirements)) };
  const found = requirements.findIndex(item => item.description === rule.description && item.excerpt === rule.excerpt);
  if (found >= 0) return { kind: "requirement", index: found };
  if (requirements.length >= 32) throw new Error("quality_filter_too_complex");
  requirements.push({ description: rule.description, excerpt: rule.excerpt }); return { kind: "requirement", index: requirements.length - 1 };
}
export function rebaseQualityFilter(rule: QualityFilter, previous: readonly { description: string; excerpt: string }[], requirements: Array<{ description: string; excerpt: string }>): QualityFilter {
  if (rule.kind === "minimum") return rule;
  if ("items" in rule) return { kind: rule.kind, items: rule.items.map(item => rebaseQualityFilter(item, previous, requirements)) };
  const condition = previous[rule.index]; if (!condition) throw new Error("quality_filter_requirement_missing");
  return freezeQualityFilter({ kind: "condition", ...condition }, requirements);
}
export function proposedFilterFromRequirements(rule: QualityFilter, requirements: readonly { description: string; excerpt: string }[]): ProposedQualityFilter {
  if (rule.kind === "minimum") return rule;
  if ("items" in rule) return { kind: rule.kind, items: rule.items.map(item => proposedFilterFromRequirements(item, requirements)) };
  const condition = requirements[rule.index];
  if (!condition) throw new Error("quality_filter_requirement_missing");
  return { kind: "condition", ...condition };
}
export type SelectionTruth = "supported" | "contradicted" | "insufficient";
export function evaluateQualityFilter(rule: QualityFilter, level: DiscoveryQualityLevel | null, requirements: readonly SelectionTruth[]): SelectionTruth {
  if (rule.kind === "minimum") {
    if (level === null) return "insufficient";
    const score = discoveryQualityScore(level); return (rule.operator === "gte" ? score >= rule.score : score > rule.score) ? "supported" : "contradicted";
  }
  if (rule.kind === "requirement") return requirements[rule.index] ?? "insufficient";
  const values = rule.items.map(item => evaluateQualityFilter(item, level, requirements));
  return rule.kind === "all" ? values.includes("contradicted") ? "contradicted" : values.every(value => value === "supported") ? "supported" : "insufficient"
    : values.includes("supported") ? "supported" : values.every(value => value === "contradicted") ? "contradicted" : "insufficient";
}
