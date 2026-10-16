import { invoke } from "./local-runtime";
import { componentsReport } from "@onecrew/runtime-contracts/discovery";

export async function checkComponentCompatibility() {
  const result = await invoke<unknown>("onecrew_components_check");
  const parsed = componentsReport.safeParse(result);
  if (!parsed.success) throw new Error("components_report_invalid");
  return parsed.data;
}
