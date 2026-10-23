import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";

import { CollapseRailIcon, ExpandRailIcon, NAV_GLYPHS } from "@/components/nav-icons";

import { OneCrewMark } from "@/components/brand";
import { LanguageSwitcher } from "@/components/language-switcher";
import { cn } from "@/lib/utils";
import { sidebarRailEdgeBarClassName } from "@/lib/sidebar-layout";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";

export type View = "leads" | "mail" | "accounts" | "connect" | "free" | "free-search";

/** Every nav item uses paired default and active assets. The OneCrew brand-ink
 * asset and the selected row band carry state without changing label weight. */
type SidebarTranslationKey =
  | "sections.discover"
  | "sections.communicate"
  | "sections.accounts"
  | "sections.free"
  | "navigation.leads"
  | "navigation.mail"
  | "navigation.accounts"
  | "navigation.connect"
  | "navigation.free"
  | "navigation.freeSearch";

type NavItem = {
  id: View;
  labelKey: SidebarTranslationKey;
  glyph: (typeof NAV_GLYPHS)[keyof typeof NAV_GLYPHS];
};

const NAV_SECTIONS: { labelKey: SidebarTranslationKey; items: NavItem[] }[] = [
  { labelKey: "sections.free", items: [
    { id: "free", labelKey: "navigation.free", glyph: NAV_GLYPHS.connect },
    { id: "free-search", labelKey: "navigation.freeSearch", glyph: NAV_GLYPHS.leads },
  ] },
  {
    labelKey: "sections.communicate",
    items: [
      { id: "mail", labelKey: "navigation.mail", glyph: NAV_GLYPHS.mail },
    ],
  },
  {
    labelKey: "sections.accounts",
    items: [
      { id: "accounts", labelKey: "navigation.accounts", glyph: NAV_GLYPHS.accounts },
    ],
  },
];

function BrandTrigger() {
  const { state, toggleSidebar } = useSidebar();
  const { t: tCommon } = useTranslation("common");
  const { t: tSidebar } = useTranslation("sidebar");
  const collapsed = state === "collapsed";

  // Collapsed, this is the same 40px tile the navigation below draws, so the
  // mark sits on the rail's centre line with every glyph under it. The label and
  // the collapse control are removed rather than shrunk to zero width: a
  // zero-width child still holds its share of the gap and pushes the mark off
  // centre, which is what put the mark right of the rail's axis.
  return (
    <button
      type="button"
      onClick={toggleSidebar}
      aria-label={collapsed ? tSidebar("toggle.expand") : tSidebar("toggle.collapse")}
      className="group/brand flex h-9 w-full cursor-pointer select-none items-center gap-3 overflow-hidden rounded-md px-2.5 text-sidebar-foreground transition-colors hover:bg-sidebar-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring group-data-[collapsible=icon]:mx-auto group-data-[collapsible=icon]:h-10! group-data-[collapsible=icon]:w-10! group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:gap-0 group-data-[collapsible=icon]:px-0 group-data-[collapsible=icon]:hover:bg-[var(--sidebar-rail-hover)]"
    >
      <OneCrewMark className="size-[30px] shrink-0 group-data-[collapsible=icon]:group-hover/brand:hidden" />
      <ExpandRailIcon className="hidden size-5 shrink-0 group-data-[collapsible=icon]:group-hover/brand:block" />
      <span className="min-w-0 truncate text-sm font-semibold group-data-[collapsible=icon]:hidden">
        {tCommon("brand.name")}
      </span>
      <CollapseRailIcon className="ml-auto size-5 shrink-0 text-[var(--sidebar-glyph)] group-data-[collapsible=icon]:hidden" />
    </button>
  );
}

export function AppSidebar({
  view,
  onNavigate,
}: {
  view: View;
  onNavigate: (view: View) => void;
}) {
  const { t } = useTranslation("sidebar");
  // Below md the sidebar is a Sheet; choosing a destination closes it. On
  // desktop openMobile is already false, so this is a no-op there.
  const { setOpenMobile } = useSidebar();
  // The primitive's border-r runs the fixed container's full height, which
  // on macOS means a line cutting up through the transparent title bar and
  // ending beside the traffic lights. The divider is drawn as an inset rule
  // instead, starting below the title bar; the same-variant border-r-0 is
  // what guarantees the primitive's own border loses the merge.
  return (
    <Sidebar
      collapsible="icon"
      className="onecrew-navigation group-data-[side=left]:border-r-0 after:pointer-events-none after:absolute after:bottom-0 after:right-0 after:top-0 after:w-px after:bg-sidebar-border"
    >
      <SidebarHeader
        className="h-14 justify-end border-b border-sidebar-border px-2 pb-1.5 pt-1.5"
      >
        <BrandTrigger />
      </SidebarHeader>
      <SidebarContent className="group-data-[collapsible=icon]:gap-1">
        {NAV_SECTIONS.map((section, sectionIndex) => (
          <SidebarGroup
            key={section.labelKey}
            className={cn(
              "px-0 py-1 group-data-[collapsible=icon]:py-0",
              sectionIndex === 0 && "pb-1 pt-3 group-data-[collapsible=icon]:pt-2",
              // Collapsed there is no room for a section name, so the section
              // break becomes the reference rail's short centred rule.
              sectionIndex > 0 &&
                "group-data-[collapsible=icon]:before:mx-auto group-data-[collapsible=icon]:before:mb-1 group-data-[collapsible=icon]:before:block group-data-[collapsible=icon]:before:h-px group-data-[collapsible=icon]:before:w-7 group-data-[collapsible=icon]:before:bg-sidebar-border group-data-[collapsible=icon]:before:content-['']",
            )}
          >
            {section.labelKey !== "sections.accounts" && <SidebarGroupLabel className="h-8 px-4 text-[11px] font-medium text-sidebar-foreground/55">
              {t(section.labelKey)}
            </SidebarGroupLabel>}
            <SidebarGroupContent>
              <SidebarMenu className="gap-0 group-data-[collapsible=icon]:gap-1">
                {section.items.map(({ id, labelKey, glyph }) => {
                  const active = view === id;
                  const label = t(labelKey);
                  const Glyph = active ? glyph.selected : glyph.rest;
                  return (
                    <SidebarMenuItem
                      key={id}
                      // The selected item's edge bar belongs to the row, not to
                      // the button: the button clips its own overflow so the
                      // label can truncate, and the bar has to sit outside the
                      // collapsed tile, flush with the rail's own edge.
                      className={cn(active && sidebarRailEdgeBarClassName)}
                    >
                      <SidebarMenuButton
                        variant="rail"
                        size="rail"
                        tooltip={label}
                        isActive={active}
                        onClick={() => {
                          setOpenMobile(false);
                          onNavigate(id);
                        }}
                        className="cursor-pointer select-none"
                      >
                        <Glyph className="size-5 shrink-0" />
                        <span className="group-data-[collapsible=icon]:hidden">{label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  );
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ))}
      </SidebarContent>
      <SidebarFooter className="border-t border-sidebar-border px-0 py-1">
        <LanguageSwitcher />
      </SidebarFooter>
    </Sidebar>
  );
}
