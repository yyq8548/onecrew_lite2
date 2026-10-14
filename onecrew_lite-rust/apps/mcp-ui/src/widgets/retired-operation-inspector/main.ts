import { App, PostMessageTransport } from "@modelcontextprotocol/ext-apps";

const app = new App(
  { name: "onecrew-retired-card", version: __MCP_UI_VERSION__ },
  {},
  { autoResize: true },
);
app.onteardown = async () => ({});
void app.connect(new PostMessageTransport(window.parent, window.parent));
