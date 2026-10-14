import { useLayoutEffect, useRef } from "react";
import { useLocation } from "react-router";
import { stripLocalePrefix } from "@/lib/site-locale";

export function RouteScrollTop() {
  const { hash, pathname } = useLocation();
  const previousPath = useRef("");

  useLayoutEffect(() => {
    // A language change keeps the same document and reading position.
    const nextPath = `${stripLocalePrefix(pathname)}${hash}`;
    if (previousPath.current === nextPath) return;
    previousPath.current = nextPath;

    const root = document.documentElement;
    const alreadyInstant = root.classList.contains("route-scroll-instant");
    root.classList.add("route-scroll-instant");
    const requestFrame = window.requestAnimationFrame?.bind(window) ?? ((callback: FrameRequestCallback) => window.setTimeout(() => callback(performance.now()), 0));
    const cancelFrame = window.cancelAnimationFrame?.bind(window) ?? window.clearTimeout.bind(window);

    let settleFrame = 0;
    const frame = requestFrame(() => {
      if (hash) {
        let targetId = hash.slice(1);
        try {
          targetId = decodeURIComponent(targetId);
        } catch {
          // Keep the literal fragment when it is not valid percent-encoding.
        }
        const target = document.getElementById(targetId);
        if (target) target.scrollIntoView();
        else window.scrollTo(0, 0);
      } else {
        window.scrollTo(0, 0);
      }
      settleFrame = requestFrame(() => {
        if (!alreadyInstant) root.classList.remove("route-scroll-instant");
      });
    });

    return () => {
      cancelFrame(frame);
      cancelFrame(settleFrame);
      if (!alreadyInstant) root.classList.remove("route-scroll-instant");
    };
  }, [hash, pathname]);

  return null;
}
