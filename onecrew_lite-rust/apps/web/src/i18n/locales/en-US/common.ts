import type { CatalogShape } from "../../catalog-shape";
import type zhCNCommon from "../zh-CN/common";

const common = {
  brand: {
    name: "onecrew_lite",
  },
  actions: {
    apply: "Apply",
    clear: "Clear",
    close: "Close",
    refresh: "Refresh",
    reset: "Reset",
    retry: "Retry",
  },
  dataTable: {
    empty: "No data",
    noResults: "No results",
    selectedCount: "{{count}} selected",
    filter: {
      ariaLabel: "Filter {{title}}",
      ariaLabelWithSelection: "Filter {{title}}, currently {{selection}}",
      clear: "Clear filters",
      datePrompt: "Choose a single date or date range",
    },
    sort: {
      ascending: "Ascending",
      descending: "Descending",
      hide: "Hide",
    },
    toolbar: {
      searchPlaceholder: "Filter…",
      view: "View",
      showColumns: "Show columns",
    },
    pagination: {
      totalRows: "{{count}} rows",
      rowsPerPage: "Rows per page",
      pageStatus: "Page {{current}} of {{total}}",
      firstPage: "First page",
      previousPage: "Previous page",
      nextPage: "Next page",
      lastPage: "Last page",
    },
  },
  providerError: {
    unavailable: "The service did not return a verifiable diagnostic.",
    notProvided: "Not provided",
    labels: {
      ownerCode: "Owner error code", stage: "Stage", platform: "Platform", protocol: "Actual protocol",
      origin: "Origin", nativeCode: "Native error code", localCode: "Local condition code",
      httpStatus: "HTTP status", rpcCode: "RPC error code",
    },
    origins: { provider: "Provider call", local: "Local check" },
    technicalDetails: "Technical details",
  },
  runtimeUnavailable: {
    title: "Local onecrew_lite service unavailable",
    description: "Click Open workbench on the OneCrew status card, or ask Codex to use OneCrew to reopen an authorized page.",
  },
  sidebar: {
    title: "Sidebar",
    mobileDescription: "Displays the mobile sidebar.",
    toggle: "Toggle sidebar",
  },
  errors: {
    clipboardUnavailable: "Clipboard access is unavailable in this environment",
    creatorIdRequired: "Creator ID is required",
  },
} as const satisfies CatalogShape<typeof zhCNCommon>;

export default common;
