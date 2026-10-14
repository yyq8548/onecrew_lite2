export type DeviceStatus = "active" | "expired" | "pending" | "revoked";

export type DeviceSummary = {
  appVersion: string;
  createdAt: string;
  deviceId: string;
  label: string;
  pairedAt: string | null;
  pairingExpiresAt: string;
  pairingId: string;
  pairingIp: string | null;
  pairingLocation: {
    country: string | null;
    region: string | null;
    city: string | null;
    colo: string | null;
  };
  platform: "linux" | "macos" | "windows";
  revokedAt: string | null;
  status: DeviceStatus;
};
