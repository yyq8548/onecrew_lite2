import { describe, expect, it, vi } from "vitest";

import { openAfterSuccessfulSubmit, runDialogMutationOnce, shouldApplyUserOpenChange } from "./manual-dialog-state";

describe("manual entry dialog lifecycle", () => {
  it("blocks a user close while pending but force-closes a successful submission", () => {
    expect(shouldApplyUserOpenChange(true, false)).toBe(false);
    expect(openAfterSuccessfulSubmit()).toBe(false);
  });

  it("uses a synchronous lock to reject a rapid second submit and invalidates after success", async () => {
    const lock = { current: false };
    const pending: boolean[] = [];
    let resolveMutation!: (value: { status: "updated" }) => void;
    const mutation = vi.fn(() => new Promise<{ status: "updated" }>((resolve) => { resolveMutation = resolve; }));
    const invalidate = vi.fn(async () => undefined);

    const first = runDialogMutationOnce({
      lock,
      onPendingChange: (value) => pending.push(value),
      mutation,
      afterSuccess: invalidate,
    });
    const second = await runDialogMutationOnce({
      lock,
      onPendingChange: (value) => pending.push(value),
      mutation,
      afterSuccess: invalidate,
    });

    expect(second).toEqual({ started: false });
    expect(mutation).toHaveBeenCalledTimes(1);
    expect(invalidate).not.toHaveBeenCalled();
    resolveMutation({ status: "updated" });
    await expect(first).resolves.toEqual({ started: true, value: { status: "updated" } });
    expect(invalidate).toHaveBeenCalledTimes(1);
    expect(pending).toEqual([true, false]);
    expect(lock.current).toBe(false);
  });
});
