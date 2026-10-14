import { readFileSync } from 'node:fs';

// The binary bundle includes this metadata beside dist, just like the source package.
const metadata: { version: string } = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
);

export const APP_VERSION = metadata.version;
