import { build } from 'esbuild';
import { mkdir, writeFile } from 'node:fs/promises';
const result = await build({stdin:{contents:"export { localizedScenario } from './src/test-support/demo-locale';",resolveDir:process.cwd(),loader:'ts'},bundle:true,platform:'node',format:'esm',write:false});
const {localizedScenario} = await import('data:text/javascript;base64,'+Buffer.from(result.outputFiles[0].text).toString('base64'));
const cases = Array.from({length:20},(_,i)=>({id:`case-${String(i+1).padStart(2,'0')}`, ...localizedScenario(i,'zh-CN')}));
await mkdir('dist/preview',{recursive:true});
await writeFile('dist/preview/20-scenarios.json',JSON.stringify({version:'2026-09-10',mode:'simulation-only',notes:'No OneCrew writes, messages, model generation or live creator assessment. initialSearch is a preview state, not a server snapshot.',cases},null,2));
