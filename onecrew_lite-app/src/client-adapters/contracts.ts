import type { LocalServiceStatus } from "@onecrew/runtime-contracts/status";
import productMetadata from "../../package.json" with { type: "json" };

export interface ClientDetection { client: "codex"; executable: string; version: string; pluginProtocol: "local_marketplace_v1"; semanticAuthority: false }
export interface ClientConfiguration { mcpServers: { onecrew_lite: { command: string; args: string[] } } }
export interface ClientPluginPackage { marketplace_root: string; plugin_root: string; version: string; target: string; binary_sha256: string; runtime_version: string }
export interface ClientAdapter {
  readonly id: string;
  detect(): Promise<ClientDetection>;
  configuration(): ClientConfiguration;
  prepare(): Promise<ClientPluginPackage>;
  install(): Promise<LocalServiceStatus["plugin"]>;
  status(refresh?: boolean): Promise<LocalServiceStatus["plugin"]>;
  uninstall(): Promise<LocalServiceStatus["plugin"]>;
}
export const runtimeCompatibility = Object.freeze({ protocol: 2, minimum: productMetadata.version, maximum: productMetadata.version });
