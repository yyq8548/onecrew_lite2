import { readFile } from "node:fs/promises";
import { z } from "zod";
import { runtimeAssetPath } from "../packaging/runtime-context.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";

export const workflowTopics = ["find-batch", "find-one", "find-replies", "find-admins", "enrich-followers", "enrich-scores", "enrich-routes", "enrich-one"] as const;
export const workflowGuideInput = z.strictObject({ topic: z.enum(workflowTopics).nullish() });
const topic = z.enum(workflowTopics), strings = z.array(z.string());
const output = z.discriminatedUnion("kind", [z.strictObject({ kind: z.literal("directory"), categories: z.array(z.strictObject({ category: z.enum(["find", "enrich"]), title: z.string(), topics: z.array(z.strictObject({ topic, title: z.string() })) })) }), z.strictObject({ kind: z.literal("guide"), guide: z.strictObject({ topic, title: z.string(), questions: strings, workflow: strings, tool_discovery_queries: strings, confirmation_boundary: z.string(), expected_output: strings, example: z.string() }) })]);
let loaded: Promise<Record<string, z.infer<typeof output>>> | undefined;
export async function readWorkflowGuide(selected: typeof workflowTopics[number] | null) {
  loaded ??= readFile(runtimeAssetPath("workflow-guides.json", new URL("./workflow-guides.json", import.meta.url))).then(bytes => z.record(z.string(), output).parse(parseApplicationJson(bytes)));
  const guides = await loaded; if (Object.keys(guides).length !== workflowTopics.length + 1 || !workflowTopics.every(name => guides[name]?.kind === "guide") || guides.directory?.kind !== "directory") throw new Error("workflow_guides_resource_invalid");
  const result = guides[selected ?? "directory"]; if (!result) throw new Error("workflow_guide_unknown_topic"); return structuredClone(result);
}
