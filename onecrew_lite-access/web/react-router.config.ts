import type { Config } from "@react-router/dev/config";

import pages from "../site/marketing-pages.json";

const localizedPaths = pages.flatMap(({ path }) => [
  path === "/" ? "/zh" : `/zh${path}`,
  path,
]);

export default {
  appDirectory: "src/site-framework",
  buildDirectory: "../site/build",
  prerender: {
    paths: localizedPaths,
    concurrency: 4,
  },
  routeDiscovery: { mode: "initial" },
  ssr: false,
} satisfies Config;
