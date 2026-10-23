import type { App } from "@modelcontextprotocol/ext-apps/react";
import { applyDocumentTheme, applyHostStyleVariables } from "@modelcontextprotocol/ext-apps/react";

type HostContext = ReturnType<App["getHostContext"]>;

export function applyHostContext(context: HostContext) {
  if (context?.theme) applyDocumentTheme(context.theme);
  if (context?.styles?.variables) applyHostStyleVariables(context.styles.variables);
}
