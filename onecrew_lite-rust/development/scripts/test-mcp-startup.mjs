// Protocol diagnostic: isolated by default; --manifest reads installed service status.
// No login, provider calls, Campaign writes, or table mutations.
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { mkdtemp, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join, dirname } from 'node:path';
import { spawnSync } from 'node:child_process';
const require = createRequire(new URL('../../apps/mcp-ui/package.json', import.meta.url));
const { Client } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/index.js')));
const { StdioClientTransport } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/stdio.js')));
const binary = resolve(process.argv[2]);
const cwd = process.argv[3] ? resolve(process.argv[3]) : dirname(binary);
const prefix = join(tmpdir(), 'onecrew_lite-mcp-probe-');
const manifestMode = process.argv[4] === '--manifest';
// Manifest mode is a read-only status check of the installed default service.
// It never stops that service or removes its data.
const data = manifestMode ? null : await mkdtemp(prefix);
const client = new Client({name:'onecrew_lite-startup-probe',version:'1'});
const config = manifestMode
  ? JSON.parse(await readFile(join(cwd,'.mcp.json'),'utf8')).mcpServers.onecrew_lite
  : {command:binary,args:['mcp','--stdio','--data-dir',data]};
const transport = new StdioClientTransport({...config,cwd:resolve(cwd,config.cwd ?? '.'),stderr:'pipe'});
let stage = 'initialize';
let stderrBytes = 0;
const started = Date.now();
// Bound this diagnostic even if an inherited pipe keeps Node alive after close.
setTimeout(() => {
  console.log(JSON.stringify({pass:false,stage,elapsedMs:Date.now()-started,resources:process.getActiveResourcesInfo()}));
  process.exit(2);
}, 45000).unref();
try {
  const connecting = client.connect(transport, {timeout:20000});
  transport.stderr?.on('data', chunk => { stderrBytes += chunk.length; });
  await connecting;
  stage = 'tools/list';
  const listed = await client.listTools({}, {timeout:20000});
  if (!listed.tools.some(t => t.name === 'onecrew_status')) throw new Error('status_tool_missing');
  stage = 'onecrew_status';
  const reply = await client.callTool({name:'onecrew_status',arguments:{}}, undefined, {timeout:30000});
  if (reply.isError || !reply.structuredContent?.running) throw new Error('status_not_running');
  console.log(JSON.stringify({pass:true,stage,elapsedMs:Date.now()-started,version:reply.structuredContent.version,stderrBytes}));
} catch (error) {
  console.log(JSON.stringify({pass:false,stage,elapsedMs:Date.now()-started,error:String(error.message).replace(/Bearer\s+\S+/gi,'Bearer <REDACTED>'),stderrBytes}));
  process.exitCode = 1;
} finally {
  await client.close();
  stage = 'transport_closed';
  console.log(JSON.stringify({stage,elapsedMs:Date.now()-started,resources:process.getActiveResourcesInfo()}));
  if (data) {
    const stopped = spawnSync(binary,['stop','--data-dir',data,'--json'],{windowsHide:true,timeout:20000,stdio:'pipe'});
    const safe = resolve(data).startsWith(resolve(prefix)) && dirname(resolve(data)) === resolve(tmpdir());
    if (stopped.status === 0 && safe) await rm(data,{recursive:true});
    else console.log(JSON.stringify({cleanup:'retained_isolated_data',stopExit:stopped.status}));
  }
}
