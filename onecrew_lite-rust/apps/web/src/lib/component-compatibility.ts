import { invoke } from "./local-runtime";
import { validateComponentsReport } from "./generated/discovery-contracts/validators/ComponentsReport.js";

export async function checkComponentCompatibility() {
  const result = await invoke<unknown>("onecrew_components_check");
  if (!validateComponentsReport(result)) throw new Error("components_report_invalid");
  return result;
}
