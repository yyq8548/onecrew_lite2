import { describe, expect, it, vi } from "vitest";

import { cleanupPendingOperatorConnection } from "./operator-account";

describe("cleanupPendingOperatorConnection", () => {
  it("does nothing during the StrictMode cleanup before a flow exists", async () => {
    const cancel = vi.fn(async () => undefined);

    await cleanupPendingOperatorConnection(null, false, cancel, []);

    expect(cancel).not.toHaveBeenCalled();
  });

  it("does not cancel a completed flow when its dialog unmounts", async () => {
    const cancel = vi.fn(async () => undefined);

    await cleanupPendingOperatorConnection("flow-completed", true, cancel, []);

    expect(cancel).not.toHaveBeenCalled();
  });

  it("cancels an unfinished GUI-owned flow when its dialog unmounts", async () => {
    const cancel = vi.fn(async () => undefined);

    await cleanupPendingOperatorConnection("flow-pending", false, cancel, []);

    expect(cancel).toHaveBeenCalledOnce();
    expect(cancel).toHaveBeenCalledWith("flow-pending");
  });

  it("retries when cancellation temporarily races an in-flight start", async () => {
    const cancel = vi.fn()
      .mockRejectedValueOnce("operator account connection operation is in progress")
      .mockResolvedValueOnce(undefined);

    await cleanupPendingOperatorConnection("flow-starting", false, cancel, [0]);

    expect(cancel).toHaveBeenCalledTimes(2);
    expect(cancel).toHaveBeenNthCalledWith(1, "flow-starting");
    expect(cancel).toHaveBeenNthCalledWith(2, "flow-starting");
  });

  it("rechecks completion before retrying a failed cancel", async () => {
    let completed = false;
    const cancel = vi.fn(async () => {
      completed = true;
      throw new Error("operator account connection operation is in progress");
    });

    await cleanupPendingOperatorConnection("flow-finishing", () => completed, cancel, [0]);

    expect(cancel).toHaveBeenCalledOnce();
  });

  it("does not retry a terminal completed-flow response", async () => {
    const cancel = vi.fn(async () => {
      throw new Error("completed operator account connections cannot be canceled");
    });

    await cleanupPendingOperatorConnection("flow-completed-remotely", false, cancel, [0, 0]);

    expect(cancel).toHaveBeenCalledOnce();
  });
});
