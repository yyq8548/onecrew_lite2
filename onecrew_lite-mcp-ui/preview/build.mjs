import { build } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
const root=fileURLToPath(new URL('../',import.meta.url));
const version=JSON.parse(readFileSync(new URL('../package.json',import.meta.url),'utf8')).version;
await build({configFile:false,root:root+'preview',plugins:[react(),tailwindcss(),viteSingleFile()],resolve:{alias:{'@mcp-ui':root+'src'}},define:{__MCP_UI_VERSION__:JSON.stringify(version)},build:{outDir:root+'dist/preview',emptyOutDir:true,assetsInlineLimit:1000000}});
