// Isolated business integration fixture; production lifecycle is tested by test:cli.
import { serve } from '@hono/node-server';
import { createApp } from '../../src/app.js';
import { TestAccess } from './test-access.js';
const { app, data, browsers, closeMcp } = await createApp(
  process.env.ONECREW_LITE_DATA_DIR,
  new TestAccess(),
);
const server = serve({ fetch: app.fetch, hostname: '127.0.0.1', port: Number(process.env.PORT) });
const close = async () => {
  browsers.closeAll();
  await closeMcp();
  if ('closeAllConnections' in server) server.closeAllConnections();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await data.close();
};
process.once('SIGTERM', () => void close());
process.once('SIGINT', () => void close());
