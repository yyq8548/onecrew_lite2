import { renderToStaticMarkup } from "react-dom/server";
import type { ColumnDef } from "@tanstack/react-table";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DataTable } from "./data-table";
import { storedFilters } from "./filter-state";

vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (key: string) => key }) }));

type Item = { id: string; stage: string; score: number };
const columns: ColumnDef<Item>[] = [
  { accessorKey: "id", header: "ID" },
  {
    accessorKey: "stage",
    header: ({ column }) => <span>facet:{column.getFacetedUniqueValues().get("engaged") ?? "absent"}</span>,
    filterFn: "arrIncludesSome",
  },
  { accessorKey: "score", header: "Score" },
];
const rows: Item[] = [
  { id: "server-first", stage: "engaged", score: 1 },
  { id: "server-second", stage: "prospect", score: 9 },
];

afterEach(() => vi.unstubAllGlobals());

describe("DataTable runtime modes", () => {
  it("uses authoritative row order and facet totals without re-filtering the received page", () => {
    const markup = renderToStaticMarkup(<DataTable
      columns={columns} data={rows} pagination={false}
      getRowId={(row) => row.id}
      columnFilters={[{ id: "stage", value: ["engaged"] }]}
      sorting={[{ id: "score", desc: true }]}
      manualFiltering manualSorting
      facetedUniqueValues={{ stage: new Map([["engaged", 205]]) }}
    />);
    expect(markup).toContain("facet:205");
    expect(markup).toContain("server-second");
    expect(markup.indexOf("server-first")).toBeLessThan(markup.indexOf("server-second"));
  });

  it("retains local sorting and faceting when server modes are omitted", () => {
    const markup = renderToStaticMarkup(<DataTable
      columns={columns} data={rows} pagination={false}
      sorting={[{ id: "score", desc: true }]}
    />);
    expect(markup).toContain("facet:1");
    expect(markup.indexOf("server-second")).toBeLessThan(markup.indexOf("server-first"));
  });

  it("restores local table filters with the same persisted key and rejects malformed storage", () => {
    const getItem = vi.fn(() => JSON.stringify([{ id: "id", value: "server-second" }]));
    vi.stubGlobal("localStorage", { getItem });
    const markup = renderToStaticMarkup(<DataTable columns={columns} data={rows} filtersStorageKey="saved" pagination={false} />);
    expect(getItem).toHaveBeenCalledWith("data-table-filters:saved");
    expect(markup).not.toContain("server-first");
    expect(markup).toContain("server-second");
    getItem.mockReturnValue(JSON.stringify([null]));
    expect(storedFilters("saved")).toBeNull();
  });

  it("keeps matching pinned rows above the server body without duplicating them", () => {
    const markup = renderToStaticMarkup(<DataTable columns={columns} data={rows} pagination={false}
      manualFiltering manualSorting getRowId={(row) => row.id} pinnedRowIds={["server-second"]}
    />);
    expect(markup.indexOf("server-second")).toBeLessThan(markup.indexOf("server-first"));
    expect(markup.match(/server-second/g)).toHaveLength(1);
  });
});
