import { describe, expect, it } from "vitest";

import { contextForDisplayMode, mergeHostContext, readHostFrame, UNSTATED_HOST_FRAME } from "./host-frame";
import { rememberHostDimensions } from "./inspector-app";

type HostContext = NonNullable<Parameters<typeof readHostFrame>[0]>;

/** The Host describes the surface this App is rendered into, and only what the
 * card actually uses is read.
 *
 * The stated container height is read now, after a page size picked by hand
 * turned out to be a guess about someone else's screen. It sizes a page of a
 * long list against the room the Host says there is; it still never caps the
 * card, which reports its own height and lets the Host size the frame. */
describe("the frame the Host states", () => {
  it("treats a Host that said nothing as unstated rather than inventing a default", () => {
    expect(readHostFrame(undefined as unknown as HostContext)).toEqual(UNSTATED_HOST_FRAME);
    expect(readHostFrame({} as HostContext)).toEqual(UNSTATED_HOST_FRAME);
  });

  it("carries the display mode, platform and input the Host reported", () => {
    const frame = readHostFrame({
      displayMode: "inline",
      availableDisplayModes: ["inline", "fullscreen"],
      platform: "mobile",
      deviceCapabilities: { touch: true },
      safeAreaInsets: { top: 44, right: 0, bottom: 34, left: 0 },
    } as HostContext);
    expect(frame.displayMode).toBe("inline");
    expect(frame.availableDisplayModes).toEqual(["inline", "fullscreen"]);
    expect(frame.platform).toBe("mobile");
    expect(frame.touch).toBe(true);
    expect(frame.safeArea).toEqual({ top: 44, right: 0, bottom: 34, left: 0 });
    // Absent capabilities never read as present.
    expect(readHostFrame({ platform: "desktop" } as HostContext).touch).toBe(false);
  });

  it("reads the room the Host stated, whether it fixed the height or capped it", () => {
    // Fixed and flexible differ in who controls the size, not in how much room
    // the Host said there is, so a page is sized from either.
    const read = (containerDimensions: unknown) =>
      readHostFrame({ containerDimensions } as HostContext).availableHeight;
    expect(read({ height: 480, width: 400 })).toBe(480);
    expect(read({ maxHeight: 640, maxWidth: 400 })).toBe(640);
    // A Host that stated only a width, nothing at all, or a meaningless height
    // gets null: the caller keeps its own conservative default instead of
    // sizing a page against a number nobody stated.
    expect(read({ maxWidth: 400 })).toBeNull();
    expect(read(undefined)).toBeNull();
    expect(read({ height: 0, width: 400 })).toBeNull();
    expect(read({ height: 0, maxHeight: 640, width: 400 })).toBe(640);
  });

  it("merges partial Host notifications without erasing the frame it already stated", () => {
    const initial = {
      displayMode: "inline",
      availableDisplayModes: ["inline", "fullscreen"],
      locale: "zh-CN",
      containerDimensions: { height: 480, width: 400 },
      safeAreaInsets: { top: 24, right: 0, bottom: 20, left: 0 },
      deviceCapabilities: { touch: true },
    } as HostContext;
    const merged = mergeHostContext(initial, { locale: "en-US" } as HostContext);
    if (!merged) throw new Error("Expected merged Host context");
    expect(merged.locale).toBe("en-US");
    expect(readHostFrame(merged)).toMatchObject({
      displayMode: "inline",
      availableHeight: 480,
      safeArea: { top: 24, bottom: 20 },
      touch: true,
    });
  });

  it("merges changed dimensions by field when a Host sends a partial dimension update", () => {
    const initial = { containerDimensions: { height: 480, width: 400 } } as HostContext;
    const merged = mergeHostContext(initial, { containerDimensions: { height: 640 } } as HostContext);
    if (!merged) throw new Error("Expected merged Host context");
    expect(merged.containerDimensions).toEqual({ height: 640, width: 400 });
    expect(initial.containerDimensions).toEqual({ height: 480, width: 400 });
  });

  it("retains the other safe-area edges when one edge changes", () => {
    const initial = { safeAreaInsets: { top: 24, right: 4, bottom: 20, left: 8 } } as HostContext;
    const merged = mergeHostContext(initial, { safeAreaInsets: { bottom: 32 } } as HostContext);
    if (!merged) throw new Error("Expected merged Host context");
    expect(merged.safeAreaInsets).toEqual({ top: 24, right: 4, bottom: 32, left: 8 });
  });

  it("does not retain the prior fixed/maximum discriminator when the Host changes sizing mode", () => {
    const initial = { containerDimensions: { height: 480, width: 400 } } as HostContext;
    const merged = mergeHostContext(initial, { containerDimensions: { maxHeight: 640, maxWidth: 420 } } as HostContext);
    if (!merged) throw new Error("Expected merged Host context");
    expect(merged.containerDimensions).toEqual({ maxHeight: 640, maxWidth: 420 });
    expect(readHostFrame(merged).availableHeight).toBe(640);
  });

  it("switches modes without carrying stale dimensions and restores the last dimensions for that mode", () => {
    const inline = { displayMode: "inline", containerDimensions: { height: 480, width: 400 } } as HostContext;
    const dimensions = {
      inline: inline.containerDimensions,
      fullscreen: { height: 900, width: 1200 },
    } as const;
    const fullscreen = contextForDisplayMode(inline, "fullscreen", dimensions);
    expect(fullscreen?.displayMode).toBe("fullscreen");
    expect(fullscreen?.containerDimensions).toEqual(dimensions.fullscreen);

    const unstatedFullscreen = contextForDisplayMode(inline, "fullscreen", {});
    expect(unstatedFullscreen?.displayMode).toBe("fullscreen");
    expect(unstatedFullscreen).not.toHaveProperty("containerDimensions");

    const restoredInline = contextForDisplayMode(fullscreen, "inline", dimensions);
    expect(restoredInline?.containerDimensions).toEqual(dimensions.inline);
  });

  it("remembers merged dimensions when a width-only notification changes the inline budget", () => {
    const initial = { displayMode: "inline", containerDimensions: { height: 480, width: 400 } } as HostContext;
    const widthOnly = rememberHostDimensions(initial, { containerDimensions: { width: 520 } } as HostContext, {});
    expect(widthOnly.inline).toEqual({ height: 480, width: 520 });

    const maxWidthOnly = rememberHostDimensions(initial, { containerDimensions: { maxWidth: 640 } } as HostContext, {});
    expect(maxWidthOnly.inline).toEqual({ height: 480, maxWidth: 640 });
  });
});
