import {build} from 'esbuild';
import {mkdir,writeFile} from 'node:fs/promises';
const result=await build({stdin:{contents:"export {flowCases,initialFlow} from './src/test-support/flow-simulator';",resolveDir:process.cwd(),loader:'ts'},bundle:true,platform:'node',format:'esm',write:false});
const {flowCases,initialFlow}=await import('data:text/javascript;base64,'+Buffer.from(result.outputFiles[0].text).toString('base64'));
await mkdir('dist/flows',{recursive:true});
await writeFile('dist/flows/15-flow-cases.json',JSON.stringify({mode:'local-simulation-only',responseDelayMs:900,rateLimitCooldownMs:3000,cases:flowCases.map(c=>({...c,initialState:initialFlow(c.id)}))},null,2));
