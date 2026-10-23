export const sidebarLayout = {
  expandedWidth: "240px",
  iconWidth: "62px",
} as const;

/** The selected item's bar, flush with the rail's own edge. It is drawn by the
 * menu row rather than by the button, because the button clips its overflow so
 * the label can truncate and the bar sits outside the collapsed tile. */
export const sidebarRailEdgeBarClassName =
  "group-data-[collapsible=icon]:before:absolute group-data-[collapsible=icon]:before:left-0 group-data-[collapsible=icon]:before:top-1/2 group-data-[collapsible=icon]:before:h-6 group-data-[collapsible=icon]:before:w-1 group-data-[collapsible=icon]:before:-translate-y-1/2 group-data-[collapsible=icon]:before:rounded-r-[4px] group-data-[collapsible=icon]:before:bg-[var(--brand-accent)] group-data-[collapsible=icon]:before:content-['']";
