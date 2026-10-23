import "i18next";

import type { DEFAULT_NAMESPACE, resources } from "./resources";

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: typeof DEFAULT_NAMESPACE;
    resources: (typeof resources)["zh-CN"];
    strictKeyChecks: true;
  }
}
