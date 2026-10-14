import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import type { App } from "@modelcontextprotocol/ext-apps/react";

export type HostContext = NonNullable<ReturnType<App["getHostContext"]>>;
export type HostDisplayMode = "inline" | "fullscreen" | "pip";
export type HostContainerDimensions = NonNullable<HostContext["containerDimensions"]>;
export type HostDimensionsByMode = Partial<Record<HostDisplayMode, HostContainerDimensions>>;

/** Merge the partial context delivered by the Host without mutating either
 * context. Nested frame values are merged too: a notification may change only
 * one dimension or one inset while leaving the rest unstated. */
export function mergeHostContext(previous: HostContext | undefined, patch: HostContext | undefined): HostContext | undefined {
  if (!previous) return patch ? { ...patch } : undefined;
  if (!patch) return { ...previous };

  const merged: NonNullable<HostContext> = { ...previous };
  for (const [key, value] of Object.entries(patch)) {
    if (value !== undefined) merged[key] = value;
  }
  if (patch.containerDimensions) {
    merged.containerDimensions = mergeContainerDimensions(previous.containerDimensions, patch.containerDimensions);
  }
  if (patch.deviceCapabilities) {
    merged.deviceCapabilities = {
      ...previous.deviceCapabilities,
      ...patch.deviceCapabilities,
    };
  }
  if (patch.safeAreaInsets) {
    merged.safeAreaInsets = {
      ...previous.safeAreaInsets,
      ...patch.safeAreaInsets,
    } as NonNullable<NonNullable<HostContext>["safeAreaInsets"]>;
  }
  if (patch.styles) {
    const styles = {
      ...previous.styles,
      ...patch.styles,
    } as NonNullable<HostContext["styles"]>;
    if (patch.styles.variables || previous.styles?.variables) {
      styles.variables = {
        ...previous.styles?.variables,
        ...patch.styles.variables,
      } as NonNullable<NonNullable<HostContext["styles"]>["variables"]>;
    }
    merged.styles = styles;
  }
  return merged;
}

function mergeContainerDimensions(
  previous: NonNullable<NonNullable<HostContext>["containerDimensions"]> | undefined,
  patch: NonNullable<NonNullable<HostContext>["containerDimensions"]>,
): NonNullable<NonNullable<HostContext>["containerDimensions"]> {
  const next = { ...previous, ...patch } as Record<string, unknown>;
  const patchValues = patch as Record<string, unknown>;
  // Switching between fixed and maximum dimensions is a replacement of that
  // axis's mode, not a reason to retain a stale discriminator from the prior
  // context. Other axes remain preserved when they were omitted by the patch.
  if (Object.hasOwn(patchValues, "height") && patchValues.height !== undefined) delete next.maxHeight;
  if (Object.hasOwn(patchValues, "maxHeight") && patchValues.maxHeight !== undefined) delete next.height;
  if (Object.hasOwn(patchValues, "width") && patchValues.width !== undefined) delete next.maxWidth;
  if (Object.hasOwn(patchValues, "maxWidth") && patchValues.maxWidth !== undefined) delete next.width;
  return next as NonNullable<NonNullable<HostContext>["containerDimensions"]>;
}

/**
 * Apply a mode accepted by the Host without carrying dimensions from another
 * mode. Hosts may answer a mode request before sending that mode's dimensions;
 * in that interval the frame is intentionally unstated. Remembered dimensions
 * are supplied by the caller when the Host has previously stated them for this
 * mode, which also lets a mode-only exit restore the inline budget.
 */
export function contextForDisplayMode(
  context: HostContext | undefined,
  mode: HostDisplayMode,
  dimensionsByMode: HostDimensionsByMode,
): HostContext | undefined {
  if (!context) return undefined;
  const dimensions = dimensionsByMode[mode];
  const next = { ...context, displayMode: mode } as HostContext;
  if (dimensions) next.containerDimensions = { ...dimensions } as HostContainerDimensions;
  else delete next.containerDimensions;
  return next;
}

/** What the Host has told this App about the frame it renders in.
 *
 * Every field here is something the Host states over the protocol. The App
 * previously measured the frame by watching its own content get cut off, which
 * only ever recovered a number the Host was already willing to say. Reading it
 * instead of inferring it is the difference between a card that adapts and one
 * that guesses. Absent fields stay null: a Host that says nothing gets the
 * intrinsic layout, never a made-up default. */
export type HostFrame = {
  displayMode: "inline" | "fullscreen" | "pip" | null;
  availableDisplayModes: readonly ("inline" | "fullscreen" | "pip")[];
  platform: "web" | "desktop" | "mobile" | null;
  /** Touch input, which decides hit target size rather than layout. */
  touch: boolean;
  safeArea: { top: number; right: number; bottom: number; left: number } | null;
  /** The room the Host says this App has, stated as a fixed height or as a
   * maximum. It sizes a page of a long list against the screen it will be read
   * on. It is not a ceiling the card imposes on itself: the card still reports
   * its own height and lets the Host size the frame. */
  availableHeight: number | null;
};

export const UNSTATED_HOST_FRAME: HostFrame = {
  displayMode: null,
  availableDisplayModes: [],
  platform: null,
  touch: false,
  safeArea: null,
  availableHeight: null,
};

export function readHostFrame(context: HostContext | undefined): HostFrame {
  if (!context) return UNSTATED_HOST_FRAME;
  return {
    displayMode: context.displayMode ?? null,
    availableDisplayModes: [...(context.availableDisplayModes ?? [])],
    platform: context.platform ?? null,
    touch: context.deviceCapabilities?.touch === true,
    safeArea: readSafeArea(context.safeAreaInsets),
    availableHeight: statedHeight(context.containerDimensions),
  };
}

/** A fixed height and a maximum height are both the Host stating how much room
 * there is; the difference is who controls the size, not how much room exists.
 * A Host that states neither gets null, never a number this App made up. */
function statedHeight(dimensions: NonNullable<HostContext>["containerDimensions"]): number | null {
  if (!dimensions) return null;
  const fixed = "height" in dimensions ? dimensions.height : undefined;
  const maximum = "maxHeight" in dimensions ? dimensions.maxHeight : undefined;
  const stated = typeof fixed === "number" && Number.isFinite(fixed) && fixed > 0 ? fixed : maximum;
  return typeof stated === "number" && Number.isFinite(stated) && stated > 0 ? stated : null;
}

function readSafeArea(insets: NonNullable<HostContext>["safeAreaInsets"]): HostFrame["safeArea"] {
  if (!insets) return null;
  const values = [insets.top, insets.right, insets.bottom, insets.left];
  if (!values.every((value) => typeof value === "number" && Number.isFinite(value) && value >= 0)) return null;
  return { top: insets.top, right: insets.right, bottom: insets.bottom, left: insets.left };
}

const HostFrameContext = createContext<HostFrame>(UNSTATED_HOST_FRAME);

/** Ask the Host for a different display mode. The Host decides. */
export type RequestDisplayMode = (mode: "inline" | "fullscreen" | "pip") => void;
const RequestDisplayModeContext = createContext<RequestDisplayMode | null>(null);

export function HostFrameProvider({ frame, onRequestDisplayMode, children }: {
  frame: HostFrame; onRequestDisplayMode?: RequestDisplayMode; children: ReactNode;
}) {
  return <HostFrameContext.Provider value={frame}>
    <RequestDisplayModeContext.Provider value={onRequestDisplayMode ?? null}>{children}</RequestDisplayModeContext.Provider>
  </HostFrameContext.Provider>;
}

export function useHostFrame(): HostFrame {
  return useContext(HostFrameContext);
}

/** The way out of a frame too small for this card's content, or null when
 * there is none.
 *
 * An inline card cannot scroll inside itself: the Host caps inline height and
 * clips whatever exceeds it, so content below the cap is not merely hard to
 * reach, it is gone. A card with genuinely more to show asks for fullscreen —
 * the one mode where it owns a scrollport — and shows a bounded summary until
 * the Host grants it. A Host that offers no fullscreen gets no such affordance,
 * because a control that cannot work is worse than none. */
export function useFullscreenEscape(): (() => void) | null {
  const frame = useHostFrame();
  const request = useContext(RequestDisplayModeContext);
  if (!request || frame.displayMode === "fullscreen") return null;
  if (!frame.availableDisplayModes.includes("fullscreen")) return null;
  return () => request("fullscreen");
}

/** Width of the element itself, not of the window.
 *
 * A media query inside an iframe reports the outer view's width, so it stays
 * wide while this card is rendered into a 320px column and the narrow layout
 * never arrives. `ResizeObserver` reads the box the card actually occupies. */
export function useCompactWidth(breakpoint = 420): [boolean, (node: HTMLElement | null) => void] {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    if (!element || typeof ResizeObserver === "undefined") return;
    const measure = (width: number) => { if (width > 0) setCompact(width < breakpoint); };
    const observer = new ResizeObserver(([entry]) => {
      if (entry) measure(entry.contentRect.width);
    });
    observer.observe(element);
    measure(element.getBoundingClientRect().width);
    return () => observer.disconnect();
  }, [element, breakpoint]);

  return [compact, useCallback((node: HTMLElement | null) => setElement(node), [])];
}
