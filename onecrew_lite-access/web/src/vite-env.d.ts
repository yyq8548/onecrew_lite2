/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ACCESS_URL: string;
  readonly VITE_BLOG_URL: string;
  readonly VITE_SITE_URL: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
