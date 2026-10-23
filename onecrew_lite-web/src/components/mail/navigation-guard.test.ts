import { describe, expect, it, vi } from "vitest";

import { requestGuardedMailNavigation } from "@/components/mail/navigation-guard";

describe("mail navigation guard", () => {
  it("continues immediately when no Mail guard is mounted", () => {
    const navigate = vi.fn();
    expect(requestGuardedMailNavigation(null, navigate)).toBe("continued");
    expect(navigate).toHaveBeenCalledOnce();
  });

  it("defers navigation until the composer guard approves it", () => {
    const navigate = vi.fn();
    let approve: (() => void) | null = null;
    const result = requestGuardedMailNavigation({
      requestLeave: (continueNavigation) => {
        approve = continueNavigation;
      },
    }, navigate);

    expect(result).toBe("guarded");
    expect(navigate).not.toHaveBeenCalled();
    expect(approve).not.toBeNull();
    (approve as unknown as () => void)();
    expect(navigate).toHaveBeenCalledOnce();
  });
});
