import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "@mcp-ui/styles/theme.css";
import { InspectorApp } from "@mcp-ui/app/inspector-app";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <InspectorApp />
  </StrictMode>,
);
