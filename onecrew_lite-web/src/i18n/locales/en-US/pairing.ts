import type { CatalogShape } from "../../catalog-shape";
import type zhCNPairing from "../zh-CN/pairing";

const pairing = {
  states: {
    checking: {
      title: "Checking connection",
      description: "Confirming whether this device can open onecrew_lite.",
    },
    idle: {
      title: "Connect your onecrew_lite account",
      description: "Confirm your account and this device in the browser, then return here to continue.",
    },
    waiting: {
      title: "Waiting for browser confirmation",
      description: "Complete sign-in and authorization in your browser.",
    },
    callback: {
      title: "Browser confirmation received",
      description: "Validating this connection. Keep onecrew_lite open.",
    },
    enrolling: {
      title: "Finishing device connection",
      description: "Establishing secure access for this device.",
    },
    expired: {
      title: "This connection has expired",
      description: "The browser confirmation exceeded its time limit.",
    },
    canceled: {
      title: "This connection was canceled",
      description: "No device connection settings were changed.",
    },
    invalidated: {
      title: "Connection no longer valid",
      description: "The remote connection for this device has been removed.",
    },
    remoteUnavailable: {
      title: "Network connection failed",
      description: "The desktop app cannot reach the onecrew_lite sync service.",
    },
    rateLimited: {
      title: "Too many attempts",
      description: "Connection attempts from this source have reached the short-term limit.",
    },
    deviceNameUnavailable: {
      title: "Computer name could not be read",
      description: "Check that a computer name is available in system settings, then start the connection again.",
    },
    deviceNameInvalid: {
      title: "Computer name cannot be used for this connection",
      description: "In system settings, use a non-empty computer name of up to 80 characters without control or text-direction characters. Some emoji count as multiple characters. Then start the connection again.",
    },
    error: {
      title: "Unable to finish connecting",
      description: "onecrew_lite could not safely confirm this connection.",
    },
  },
  actions: {
    begin: "Sign in and connect in browser",
    reopen: "Reopen browser",
    restart: "Start again",
    rebind: "Connect again",
    retryNetwork: "Check network again",
    retry: "Check again",
    cancel: "Cancel this connection",
  },
  progress: {
    checking: "Checking connection",
    callback: "Validating browser response",
    enrolling: "Finishing device connection",
  },
  alerts: {
    expired: {
      title: "Connection not completed",
      description: "Start again and complete the new connection flow in your browser.",
    },
    canceled: {
      title: "Connection not completed",
      description: "Start the connection again when you are ready to continue.",
    },
    invalidated: {
      title: "Device must reconnect",
      description: "The previous sync credential is no longer valid. Reconnect to continue using onecrew_lite and MCP.",
    },
    remoteUnavailable: {
      title: "Check your network or proxy",
      description: "Make sure your proxy, TUN, or current network can reach onecrew_lite, then reconnect after the network recovers.",
    },
    rateLimited: {
      title: "Try again later",
      description: "onecrew_lite will not reopen browser authorization during the cooldown. Check again later.",
    },
    deviceNameUnavailable: {
      title: "Computer name could not be read",
      description: "Check that a computer name is available in system settings, then start the connection again.",
    },
    deviceNameInvalid: {
      title: "Computer name cannot be used for this connection",
      description: "In system settings, use a non-empty computer name of up to 80 characters without control or text-direction characters. Some emoji count as multiple characters. Then start the connection again.",
    },
    error: {
      title: "Connection not completed",
      description: "Start again. If the problem continues, try again later.",
    },
  },
} as const satisfies CatalogShape<typeof zhCNPairing>;

export default pairing;
