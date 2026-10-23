import { build } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { fileURLToPath } from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
await build({configFile:false,root:root+'preview/flows',plugins:[react(),tailwindcss(),viteSingleFile()],resolve:{alias:{'@mcp-ui':root+'src'}},build:{outDir:root+'dist/flows',emptyOutDir:true,assetsInlineLimit:1000000}});
