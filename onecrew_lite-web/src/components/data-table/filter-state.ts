import { useState } from "react";
import type { ColumnFiltersState } from "@tanstack/react-table";

export function storedFilters(key: string): ColumnFiltersState | null {
  try {
    const raw = localStorage.getItem(`data-table-filters:${key}`);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) && parsed.every((filter) =>
      filter && typeof filter === "object" && typeof filter.id === "string" && "value" in filter)
      ? parsed as ColumnFiltersState : null;
  } catch {
    return null;
  }
}

/** The query owner can restore filters before its first request, while local
 * tables retain the same storage format and update behavior. */
export function usePersistedColumnFilters(key?: string, initial: ColumnFiltersState = []) {
  const [filters, setFilters] = useState<ColumnFiltersState>(
    () => (key ? storedFilters(key) : null) ?? initial,
  );
  const onChange: typeof setFilters = (updater) => {
    setFilters((current) => {
      const next = typeof updater === "function" ? updater(current) : updater;
      if (key) {
        try {
          localStorage.setItem(`data-table-filters:${key}`, JSON.stringify(next));
        } catch {
          // Storage failure affects restoration only; the current filter works.
        }
      }
      return next;
    });
  };
  return [filters, onChange] as const;
}
