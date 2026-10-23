import { invoke } from "@/lib/local-runtime";
import { queryOptions } from "@tanstack/react-query";

export type ToolkitDirectoryToolkit =
  | "x"
  | "instagram"
  | "telegram"
  | "youtube"
  | "onecrew"
  | "email";

export type ToolkitDirectoryOperation = {
  id: string;
  title: string;
  summary: string;
};

export type ToolkitDirectory = {
  id: string;
  toolkit: ToolkitDirectoryToolkit;
  title: string;
  summary: string;
  purpose: string;
  connection_model: string;
  identifier_semantics: string[];
  capability_categories: string[];
  gotchas: string[];
  operations: ToolkitDirectoryOperation[];
};

export const TOOLKIT_DIRECTORY_QUERY_KEY = ["toolkit-directory"] as const;

export function listToolkitDirectory() {
  return invoke<ToolkitDirectory[]>("toolkit_directory");
}

export function toolkitDirectoryQueryOptions(enabled: boolean) {
  return queryOptions({
    queryKey: TOOLKIT_DIRECTORY_QUERY_KEY,
    queryFn: listToolkitDirectory,
    enabled,
    retry: false,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });
}
