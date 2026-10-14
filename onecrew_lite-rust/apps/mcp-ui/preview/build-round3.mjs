import { build } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { fileURLToPath } from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
await build({configFile:false,root:root+'preview/round3',plugins:[react(),tailwindcss(),viteSingleFile()],resolve:{alias:{'@mcp-ui':root+'src'}},build:{outDir:root+'dist/round3',emptyOutDir:true,assetsInlineLimit:1000000}});
