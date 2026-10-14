import { api } from '@/lib/api';

/** No periodic table reads. Server sends an invalidation only after publication. */
export async function watchCreatorUpdates(
  tableId: string,
  signal: AbortSignal,
  changed: () => void,
) {
  const response = await api['dynamic-tables'][':tableId']['creator-updates'].$get(
    { param: { tableId } },
    { init: { signal } },
  );
  if (!response.ok || !response.body) throw new Error('Creator update stream unavailable');
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  try {
    while (!signal.aborted) {
      const chunk = await reader.read();
      if (chunk.done) return;
      buffer += decoder.decode(chunk.value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      if (lines.some((line) => line === '{"changed":true}')) changed();
      if (buffer.length > 1024) throw new Error('Invalid creator update stream');
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}
