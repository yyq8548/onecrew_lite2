import type { CatalogShape } from "../../catalog-shape";
import type zhCNConnect from "../zh-CN/connect";

const connect = {
  page: {
    eyebrow: "onecrew_lite × Agent",
    title: "Let Agent work directly with onecrew_lite",
    description: "Use connected accounts to discover, manage, and reach Creators while keeping execution local and every capability explicitly authorized.",
  },
  capabilities: {
    discover: {
      title: "Discover Creators",
      description: "Find creators by market and topic",
    },
    creators: {
      title: "Maintain onecrew_lite",
      description: "Keep contacts and conversations together",
    },
    outreach: {
      title: "Run outreach",
      description: "Send DMs or email only after confirmation",
    },
  },
  service: {
    label: "Local MCP",
    supportCode: "Support code: {{code}}",
    states: {
      stopped: {
        badge: "Not started",
        title: "Ready to start",
        description: "Start local MCP to make both connection methods below available.",
        action: "Start local MCP",
      },
      starting: {
        badge: "Starting",
        title: "Starting local MCP",
        description: "Completing the MCP protocol check.",
        action: "Starting…",
      },
      healthy: {
        badge: "Healthy",
        title: "Agent connection is ready",
        description: "Codex and standard MCP clients can now connect.",
        action: "Stop local MCP",
      },
      degraded: {
        badge: "Connection issue",
        title: "Connection temporarily degraded",
        description: "The health check failed and onecrew_lite is recovering automatically.",
        action: "Stop local MCP",
      },
      recovering: {
        badge: "Recovering",
        title: "Restoring the connection",
        description: "Re-establishing the local MCP service.",
        action: "Recovering…",
      },
      stopping: {
        badge: "Stopping",
        title: "Stopping local MCP",
        description: "Closing the local MCP service safely.",
        action: "Stopping…",
      },
      faulted: {
        badge: "Needs attention",
        title: "Service needs attention",
        description: "Review the error details, then restart local MCP.",
        action: "Restart local MCP",
      },
    },
    toasts: {
      stopped: "Local MCP stopped",
      started: "Local MCP started and passed its protocol check",
      startFailed: "Local MCP failed to start",
    },
  },
  access: {
    eyebrow: "Connection methods",
    title: "Choose how an agent connects",
    description: "Codex uses the plugin; other clients use a one-time MCP guide",
  },
  codex: {
    name: "Codex",
    subtitle: "GPT Codex · onecrew_lite plugin",
    lifecycle: {
      desktopOnly: {
        badge: "Desktop only",
        title: "Codex checks require the desktop app",
        description: "Manage the Codex plugin in the onecrew_lite desktop app.",
      },
      stagingFailed: {
        badge: "Generation failed",
        title: "onecrew_lite plugin generation failed",
        description: "This service run did not produce a usable plugin package, so onecrew_lite will not reuse the previous run's version state. Restart the local service and try again.",
      },
      waiting: {
        badge: "Waiting for service",
        title: "Waiting for local service",
        description: "After startup, onecrew_lite generates the plugin for this connection and checks the version actually installed in Codex.",
      },
      checking: {
        badge: "Checking",
        title: "Checking the onecrew_lite plugin",
        description: "Generating or verifying the onecrew_lite plugin version for this connection.",
      },
      installFailedDescription: "Installation did not complete. Try again; an existing version will not be reported as current.",
    },
    pluginStates: {
      transportFailed: {
        badge: "Codex check unavailable",
        title: "Codex plugin status is temporarily unavailable",
        description: "onecrew_lite cannot currently read the Codex plugin status and will not treat a cached version as current.",
        supportingDescription: "Local MCP is unaffected. Try the check again shortly.",
      },
      unrelatedMarketplaceHealthy: {
        badge: "Codex setup needs attention",
        title: "Codex cannot read the plugin list",
        description: "Local MCP and onecrew_lite are healthy. Another Codex marketplace, “{{marketplace}}”, is invalid, so onecrew_lite cannot verify its plugin version. Remove or repair that marketplace and check again.",
        supportingDescription: "This only affects the Codex plugin check; other MCP clients can still connect to onecrew_lite.",
      },
      unrelatedMarketplace: {
        badge: "Codex setup needs attention",
        title: "Codex cannot read the plugin list",
        description: "Another Codex marketplace, “{{marketplace}}”, is invalid, so onecrew_lite cannot verify its plugin version. Remove or repair that marketplace and check again.",
        supportingDescription: "This comes from the local Codex configuration and does not mean the onecrew_lite marketplace is damaged.",
      },
      localMarketplaceInvalid: {
        badge: "Marketplace issue",
        title: "onecrew_lite marketplace is invalid",
        description: "The local onecrew_lite marketplace registered in Codex is invalid, so its plugin version cannot be checked or updated.",
        supportingDescription: "Restart local MCP to regenerate the marketplace, then check again.",
      },
      genericFailure: {
        badge: "Codex check unavailable",
        title: "Codex plugin status could not be read",
        description: "The Codex plugin check failed, so the onecrew_lite plugin version cannot currently be verified.",
        supportingDescription: "Local MCP is unaffected. Try the check again shortly.",
      },
      notDetected: {
        badge: "Not detected",
        title: "Codex was not detected",
        description: "Install Codex on this device to connect it to onecrew_lite.",
        supportingDescription: "",
      },
      notReady: {
        badge: "Not ready",
        title: "onecrew_lite plugin has not been generated",
        description: "The onecrew_lite plugin package is not ready. Restart the local service and try again.",
        supportingDescription: "",
      },
      notInstalled: {
        badge: "Available to install",
        title: "Install the onecrew_lite plugin",
        description: "Install the onecrew_lite plugin to use accounts, Creators, and connection capabilities in a new conversation.",
        supportingDescription: "",
      },
      updateAvailable: {
        badge: "Update available",
        title: "Update the onecrew_lite plugin",
        description: "A newer onecrew_lite plugin is available. The update takes effect in a new conversation.",
        supportingDescription: "",
      },
      notRegistered: {
        badge: "Repair needed",
        title: "Repair the onecrew_lite plugin connection",
        description: "The plugin version is present, but the local onecrew_lite marketplace is not registered.",
        supportingDescription: "",
      },
      ready: {
        badge: "Ready",
        title: "onecrew_lite plugin is ready",
        description: "The onecrew_lite plugin is current and loads its connection capabilities in new conversations.",
        supportingDescription: "",
      },
    },
    actions: {
      install: "Install onecrew_lite plugin",
      update: "Update onecrew_lite plugin",
      repair: "Repair plugin connection",
    },
    versions: {
      ariaLabel: "Current and latest onecrew_lite plugin versions",
      current: "Current",
      latest: "Latest",
      notInstalled: "Not installed",
    },
    note: "Plugin changes take effect in new Codex conversations",
    buttons: {
      upToDate: "Up to date",
      recheck: "Check again",
      installing: "Installing…",
      unavailable: "Unavailable",
      configureAfterStart: "Start service to configure",
    },
    toasts: {
      repaired: "Codex plugin connection repaired",
      upToDate: "onecrew_lite plugin is current",
      installed: "onecrew_lite plugin installed or updated",
      restartConversation: "Start a new Codex conversation to use it.",
      installFailed: "Plugin installation failed",
    },
    marketplace: {
      unknown: "Unknown marketplace",
    },
  },
  standard: {
    name: "Other Agent",
    subtitle: "Standard MCP clients",
    states: {
      ready: {
        badge: "Available",
        title: "Generate a one-time connection guide",
        description: "Copy it into a client that supports Streamable HTTP and follow the included steps.",
        note: "Valid for 5 minutes and usable only once",
      },
      waiting: {
        badge: "Waiting for service",
        title: "Waiting for local service",
        description: "Start the service to generate a secure short-lived MCP connection guide.",
        note: "Connection details are never exposed long term",
      },
    },
    buttonAfterStart: "Start service to generate",
  },
  setupPrompt: {
    generating: "Generating…",
    copied: "Connection guide copied",
    regenerate: "Generate a new connection guide",
    generate: "Generate and copy connection guide",
    copyFailed: "Could not copy the MCP connection guide",
  },
  footer: {
    privacy: "Accounts and credentials stay in onecrew_lite; local Agent receives only the capabilities currently authorized",
  },
} as const satisfies CatalogShape<typeof zhCNConnect>;

export default connect;
