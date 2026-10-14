const IP_ADDRESS_PATTERN = /^[0-9A-Fa-f:.]{3,64}$/;

export function normalizeIpAddress(value: string | null | undefined) {
  const normalized = value?.trim() ?? "";
  return IP_ADDRESS_PATTERN.test(normalized) ? normalized : null;
}
