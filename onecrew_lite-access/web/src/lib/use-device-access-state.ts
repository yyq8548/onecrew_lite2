import { useCallback, useEffect, useRef, useState } from "react";

import {
  AccessApiError,
  getAccessDeviceStatus,
  listAccessDevices,
  type AccessErrorCode,
  type DeviceSummary,
} from "@/lib/access-api";

export type DeviceFailureCode = AccessErrorCode | "unknown";

type DeviceAccessState = {
  devices: DeviceSummary[];
  error: DeviceFailureCode | null;
  nextCursor: string | null;
  phase: "idle" | "loading" | "loading_more" | "refreshing" | "status";
  refreshingDeviceId: string | null;
};

const INITIAL_STATE: DeviceAccessState = {
  devices: [],
  error: null,
  nextCursor: null,
  phase: "loading",
  refreshingDeviceId: null,
};

export function useDeviceAccessState({
  onAuthenticationRequired,
  ownerUserId,
}: {
  onAuthenticationRequired: () => void;
  ownerUserId: string;
}) {
  const [state, setState] = useState<DeviceAccessState>(INITIAL_STATE);
  const mounted = useRef(false);
  const requestSequence = useRef(0);
  const controllers = useRef(new Set<AbortController>());

  const reportFailure = useCallback(
    (error: unknown): DeviceFailureCode => {
      const code = error instanceof AccessApiError ? error.code : undefined;
      if (code === "authentication_required") {
        onAuthenticationRequired();
      }
      return code ?? "unknown";
    },
    [onAuthenticationRequired],
  );

  const refresh = useCallback(async () => {
    const requestId = ++requestSequence.current;
    const controller = new AbortController();
    controllers.current.add(controller);
    setState((current) => ({
      ...current,
      error: null,
      phase: current.devices.length === 0 ? "loading" : "refreshing",
      refreshingDeviceId: null,
    }));

    try {
      const page = await listAccessDevices(null, { signal: controller.signal });
      if (mounted.current && requestId === requestSequence.current) {
        setState({
          devices: page.devices,
          error: null,
          nextCursor: page.nextCursor,
          phase: "idle",
          refreshingDeviceId: null,
        });
      }
      return page;
    } catch (error) {
      if (controller.signal.aborted) return null;
      if (mounted.current && requestId === requestSequence.current) {
        const failure = reportFailure(error);
        setState((current) => ({
          ...current,
          error: failure,
          phase: "idle",
          refreshingDeviceId: null,
        }));
      }
      return null;
    } finally {
      controllers.current.delete(controller);
    }
  }, [reportFailure]);

  useEffect(() => {
    mounted.current = true;
    setState(INITIAL_STATE);
    void refresh();
    return () => {
      mounted.current = false;
      requestSequence.current += 1;
      for (const controller of controllers.current) controller.abort();
      controllers.current.clear();
    };
  }, [ownerUserId, refresh]);

  const loadMore = useCallback(async () => {
    if (!state.nextCursor || state.phase !== "idle") return null;
    const cursor = state.nextCursor;
    const requestId = ++requestSequence.current;
    const controller = new AbortController();
    controllers.current.add(controller);
    setState((current) => ({ ...current, error: null, phase: "loading_more" }));

    try {
      const page = await listAccessDevices(cursor, { signal: controller.signal });
      if (mounted.current && requestId === requestSequence.current) {
        setState((current) => {
          const devices = [...current.devices];
          const known = new Set(devices.map((device) => device.deviceId));
          for (const device of page.devices) {
            if (!known.has(device.deviceId)) devices.push(device);
          }
          return {
            devices,
            error: null,
            nextCursor: page.nextCursor,
            phase: "idle",
            refreshingDeviceId: null,
          };
        });
      }
      return page;
    } catch (error) {
      if (controller.signal.aborted) return null;
      if (mounted.current && requestId === requestSequence.current) {
        const failure = reportFailure(error);
        setState((current) => ({ ...current, error: failure, phase: "idle" }));
      }
      return null;
    } finally {
      controllers.current.delete(controller);
    }
  }, [reportFailure, state.nextCursor, state.phase]);

  const refreshDevice = useCallback(
    async (deviceId: string) => {
      if (state.phase !== "idle") return null;
      const requestId = ++requestSequence.current;
      const controller = new AbortController();
      controllers.current.add(controller);
      setState((current) => ({
        ...current,
        error: null,
        phase: "status",
        refreshingDeviceId: deviceId,
      }));

      try {
        const device = await getAccessDeviceStatus(deviceId, {
          signal: controller.signal,
        });
        if (mounted.current && requestId === requestSequence.current) {
          setState((current) => ({
            ...current,
            devices: current.devices.map((candidate) =>
              candidate.deviceId === device.deviceId ? device : candidate,
            ),
            error: null,
            phase: "idle",
            refreshingDeviceId: null,
          }));
        }
        return device;
      } catch (error) {
        if (controller.signal.aborted) return null;
        if (mounted.current && requestId === requestSequence.current) {
          const failure = reportFailure(error);
          setState((current) => ({
            ...current,
            error: failure,
            phase: "idle",
            refreshingDeviceId: null,
          }));
        }
        return null;
      } finally {
        controllers.current.delete(controller);
      }
    },
    [reportFailure, state.phase],
  );

  const replaceDevice = useCallback((device: DeviceSummary) => {
    if (!mounted.current) return;
    setState((current) => ({
      ...current,
      devices: current.devices.map((candidate) =>
        candidate.deviceId === device.deviceId ? device : candidate,
      ),
    }));
  }, []);

  const removeDevice = useCallback((deviceId: string) => {
    if (!mounted.current) return;
    setState((current) => ({
      ...current,
      devices: current.devices.filter((device) => device.deviceId !== deviceId),
    }));
  }, []);

  return { loadMore, refresh, refreshDevice, removeDevice, replaceDevice, state };
}
