import accountsActive from "@/assets/nav-icons/contacts-active.svg";
import accountsDefault from "@/assets/nav-icons/contacts-default.svg";
import connectActive from "@/assets/nav-icons/connect-active.svg";
import connectDefault from "@/assets/nav-icons/connect-default.svg";
import inboxActive from "@/assets/nav-icons/conversations-active.svg";
import inboxDefault from "@/assets/nav-icons/conversations-default.svg";
import leadsActive from "@/assets/nav-icons/creators-active.svg";
import leadsDefault from "@/assets/nav-icons/creators-default.svg";
import mailActive from "@/assets/nav-icons/inbox-active.svg";
import mailDefault from "@/assets/nav-icons/inbox-default.svg";

type GlyphProps = { className?: string };

// AppSidebar chooses one file from each default/active pair. Resting assets use
// neutral ink; selected assets use OneCrew brand ink and state-specific geometry.
function createNavigationGlyph(source: string) {
  return function NavigationGlyph({ className }: GlyphProps) {
    return <img src={source} alt="" aria-hidden="true" draggable={false} className={className} />;
  };
}

export const NAV_GLYPHS = {
  leads: {
    rest: createNavigationGlyph(leadsDefault),
    selected: createNavigationGlyph(leadsActive),
  },
  inbox: {
    rest: createNavigationGlyph(inboxDefault),
    selected: createNavigationGlyph(inboxActive),
  },
  mail: {
    rest: createNavigationGlyph(mailDefault),
    selected: createNavigationGlyph(mailActive),
  },
  accounts: {
    rest: createNavigationGlyph(accountsDefault),
    selected: createNavigationGlyph(accountsActive),
  },
  connect: {
    rest: createNavigationGlyph(connectDefault),
    selected: createNavigationGlyph(connectActive),
  },
} as const;

export function CollapseRailIcon({ className }: GlyphProps) {
  return (
    <svg viewBox="0 0 20 20" className={className} fill="none" aria-hidden focusable="false">
      <rect x="2" y="3" width="16" height="14" rx="4" stroke="currentColor" strokeWidth="1.5" />
      <path d="M6 5.80005V14.2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

export function ExpandRailIcon({ className }: GlyphProps) {
  return (
    <svg viewBox="0 0 20 20" className={className} fill="none" aria-hidden focusable="false">
      <rect x="2" y="3" width="16" height="14" rx="4" stroke="currentColor" strokeWidth="1.5" />
      <path d="M14 5.80005V14.2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}
