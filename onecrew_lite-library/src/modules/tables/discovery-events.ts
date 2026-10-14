/** Invalidations contain no table data and are isolated by tenant and table. */
const listeners = new Map<string, Set<() => void>>();
const key = (tenant: string, table: string) => JSON.stringify([tenant, table]);

export function onDiscoveryChanged(tenant: string, table: string, listener: () => void) {
  const id = key(tenant, table);
  const group = listeners.get(id) ?? new Set<() => void>();
  group.add(listener);
  listeners.set(id, group);
  return () => {
    group.delete(listener);
    if (!group.size) listeners.delete(id);
  };
}

export function notifyDiscoveryChanged(tenant: string, table: string) {
  for (const listener of listeners.get(key(tenant, table)) ?? []) listener();
}
