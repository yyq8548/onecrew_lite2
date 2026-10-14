import { renderToStaticMarkup } from "react-dom/server";
import { getCoreRowModel, getFilteredRowModel, getPaginationRowModel, useReactTable } from "@tanstack/react-table";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it } from "vitest";
import enUS from "@/i18n/locales/en-US/common";
import zhCN from "@/i18n/locales/zh-CN/common";
import { DataTablePagination } from "./pagination";

function PaginationFixture({ count, pageIndex = 0, filter = "" }: {
  count: number;
  pageIndex?: number;
  filter?: string;
}) {
  const table = useReactTable({
    data: Array.from({ length: count }, (_, id) => ({ name: `account-${id}` })),
    columns: [{ accessorKey: "name", filterFn: "includesString" }],
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: { pageIndex, pageSize: 10 },
      columnFilters: [{ id: "name", value: filter }],
    },
  });
  return <DataTablePagination table={table} />;
}

describe.each([
  ["zh-CN", zhCN],
  ["en-US", enUS],
] as const)("DataTable pagination in %s", (locale, common) => {
  async function render(props: Parameters<typeof PaginationFixture>[0]) {
    const i18n = i18next.createInstance();
    await i18n.init({ lng: locale, defaultNS: "common", resources: { [locale]: { common } } });
    const markup = renderToStaticMarkup(<I18nextProvider i18n={i18n}><PaginationFixture {...props} /></I18nextProvider>);
    const navigation = [...markup.matchAll(/<button\b([^>]*)>([\s\S]*?)<\/button>/g)]
      .filter((match) => match[2].includes('<span class="sr-only">'))
      .map((match) => match[1].includes('disabled=""'));
    return { markup, navigation, i18n };
  }

  it.each([
    { count: 0 },
    { count: 21, pageIndex: 2, filter: "missing" },
  ])("shows zero pages and disables every navigation button for empty results: %j", async (props) => {
    const { markup, navigation, i18n } = await render(props);
    expect(markup).toContain(i18n.t("dataTable.pagination.pageStatus", { current: 0, total: 0 }));
    expect(markup).toContain(i18n.t("dataTable.pagination.totalRows", { count: 0 }));
    expect(navigation).toEqual([true, true, true, true]);
  });

  it.each([
    [0, [true, true, false, false]],
    [1, [false, false, false, false]],
    [2, [false, false, true, true]],
  ])("preserves populated page %i and its navigation boundaries", async (pageIndex, disabled) => {
    const { markup, navigation, i18n } = await render({ count: 21, pageIndex });
    expect(markup).toContain(i18n.t("dataTable.pagination.pageStatus", { current: pageIndex + 1, total: 3 }));
    expect(navigation).toEqual(disabled);
  });
});
