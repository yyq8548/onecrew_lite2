#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, open, readFile, realpath, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { isSea } from 'node:sea';
import { setTimeout as delay } from 'node:timers/promises';
import { probeRuntimeLock, readRuntime, runtimePaths, verifyRuntime } from './runtime.js';
import { APP_VERSION } from './version.js';

const commandNames = [
  'start',
  'stop',
  'restart',
  'status',
  'web',
  'serve',
  'mcp',
  'configure-plugin',
] as const;
type Command = (typeof commandNames)[number];

const generalHelp = `onecrew_lite — local onecrew_lite service and MCP bridge

Usage:
  onecrew_lite <command> [options]

Commands:
  start             Start the service in the background
  stop              Gracefully stop the background service
  restart           Gracefully stop, then start the service in the background
  status            Show the running service URL and data profile
  web               Start/reuse the service and print its Library URL as JSON
  serve             Run the service in the foreground
  mcp               Run the foreground stdio MCP bridge
  configure-plugin  Point a onecrew_lite Codex plugin at this executable
  help              Show help for a command

Options:
  --data-dir PATH  Select the data profile (also ONECREW_LITE_DATA_DIR)
  --plugin-dir PATH  Plugin directory for configure-plugin
  --port PORT      Service port; start at 8191 and try the next 19 ports
                   (applies to serve, start, and restart)
  -h, --help       Show general or command help
  -v, --version    Show the application version

Default data profiles:
  macOS    ~/Library/Application Support/onecrew_lite_tables
  Windows  %LOCALAPPDATA%/onecrew_lite_tables
  Linux    $XDG_DATA_HOME/onecrew_lite_tables or ~/.local/share/onecrew_lite_tables

The direct HTTP MCP endpoint is /mcp at the service URL printed by status.
Update a fixed-client URL if the service port changes.

Run “onecrew_lite help mcp” for MCP bridge details.`;

const commandHelp: Record<Command, string> = {
  'configure-plugin': `onecrew_lite configure-plugin --plugin-dir PATH [--data-dir PATH]

Writes .mcp.json in an existing onecrew_lite Codex plugin directory using this
executable's absolute path. --data-dir optionally selects the service profile.
The plugin contains only instructions and connection settings. This command
does not install the plugin, change PATH, or start or stop the service.

Run this after placing the executable in its installation directory, then
install the configured plugin through Codex. If the executable moves, configure
and reinstall the plugin again so Codex's cached copy uses the new path.`,
  start: `onecrew_lite start — start the local service in the background

The command returns after the service is ready. If it is already running,
the existing instance is reported. --data-dir selects the profile; --port
sets the starting port (8191 by default, with the next 19 ports tried).`,
  stop: `onecrew_lite stop — gracefully stop the background service

The service is asked to shut down and the command waits for it to exit.
--data-dir selects the profile.`,
  restart: `onecrew_lite restart — gracefully stop and start the background service

The existing instance is stopped before a new background instance starts.
--data-dir selects the profile; --port sets the starting port (8191 by
default, with the next 19 ports tried).`,
  status: `onecrew_lite status — show service state, URL, and data profile

The command reads the selected profile's runtime descriptor and verifies the
instance. --data-dir selects the profile.`,
  web: `onecrew_lite web — start/reuse the service and return its Library URL

Returns secret-free JSON { url } after verifying the running instance.
It does not open a browser, perform login, or create a table. The desktop
launcher opens this URL beside the onecrew_lite workbench. --data-dir selects
the profile; --port selects the starting port when a service must start.`,
  serve: `onecrew_lite serve — run the local service in the foreground

The process stays attached to the terminal until it is stopped. --data-dir
selects the profile; --port sets the starting port (8191 by default, with
the next 19 ports tried).`,
  mcp: `onecrew_lite mcp — run the foreground stdio MCP bridge

This bridge uses JSON-RPC on stdin/stdout, is started and owned by any
compatible MCP client, and forwards tools to the already-running local HTTP
MCP service. It does not start the local service, open a browser, install
skills or plugins, or perform login. --data-dir (or ONECREW_LITE_DATA_DIR)
selects the profile; defaults are ~/Library/Application Support/onecrew_lite_tables on
macOS, %LOCALAPPDATA%/onecrew_lite_tables on Windows, and $XDG_DATA_HOME/onecrew_lite_tables or
~/.local/share/onecrew_lite_tables on Linux. Inside that selected profile, run/server.json
provides the current service URL. The bridge verifies that instance and
follows port changes. Start the local service first.

The direct HTTP /mcp endpoint is at the actual service URL shown by onecrew_lite
status; update a fixed-client URL when the port changes. --port does not
select an MCP port and applies only when starting the local service.

Examples:
  /path/to/installed/onecrew_lite mcp
  ONECREW_LITE_DATA_DIR=/path/to/profile /path/to/installed/onecrew_lite mcp`,
};

function isCommand(value: string | undefined): value is Command {
  return value !== undefined && (commandNames as readonly string[]).includes(value);
}

function printHelp(topic?: string) {
  if (!topic) {
    console.log(generalHelp);
    return;
  }
  if (!isCommand(topic)) throw new Error(`Unknown help topic: ${topic}`);
  console.log(commandHelp[topic]);
}

let values: {
  'data-dir'?: string;
  'plugin-dir'?: string;
  port?: string;
  help?: boolean;
  version?: boolean;
};
let positionals: string[];
try {
  ({ values, positionals } = parseArgs({
    options: {
      'data-dir': { type: 'string' },
      'plugin-dir': { type: 'string' },
      port: { type: 'string' },
      help: { type: 'boolean', short: 'h' },
      version: { type: 'boolean', short: 'v' },
    },
    allowPositionals: true,
    strict: true,
  }));
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
  values = {};
  positionals = [];
}

const command = positionals[0];
let paths: ReturnType<typeof runtimePaths>;

async function configurePlugin() {
  if (!values['plugin-dir']) throw new Error('configure-plugin requires --plugin-dir PATH');
  const pluginDir = await realpath(values['plugin-dir']);
  const manifest = JSON.parse(
    await readFile(path.join(pluginDir, '.codex-plugin', 'plugin.json'), 'utf8'),
  );
  if (manifest.name !== 'onecrew_lite-tables' || manifest.mcpServers !== './.mcp.json') {
    throw new Error('Expected a onecrew_lite Codex plugin with mcpServers set to ./.mcp.json');
  }
  const args = isSea() ? ['mcp'] : [fileURLToPath(import.meta.url), 'mcp'];
  if (values['data-dir']) args.push('--data-dir', path.resolve(values['data-dir']));
  const config = {
    mcpServers: { onecrew_lite_tables: { command: process.execPath, args } },
  };
  const configPath = path.join(pluginDir, '.mcp.json');
  await writeFile(configPath, `${JSON.stringify(config, null, 2)}\n`);
  console.log(`Configured ${configPath}`);
}

async function start(quiet = false) {
  let current;
  try {
    current = await readRuntime();
  } catch {
    const lock = await probeRuntimeLock();
    if (lock === 'locked')
      throw new Error('onecrew_lite is starting or unavailable; inspect the profile');
    return launch(quiet);
  }
  if (current) {
    try {
      await verifyRuntime(current);
      if (!quiet) console.log(`already running (${current.pid}) at ${current.url}`);
      return;
    } catch {
      const lock = await probeRuntimeLock();
      if (lock !== 'locked') return launch(quiet);
      throw new Error(`onecrew_lite is running or unavailable; inspect ${paths.logFile}`);
    }
  }
  if ((await probeRuntimeLock()) === 'locked')
    throw new Error('onecrew_lite is starting or unavailable; inspect the profile');
  await launch(quiet);
}

async function launch(quiet = false) {
  await mkdir(paths.dataDir, { recursive: true, mode: 0o700 });
  const log = await open(paths.logFile, 'a', 0o600);
  const entry = fileURLToPath(new URL('./cli.js', import.meta.url));
  const args = ['serve', '--data-dir', paths.dataDir];
  if (!isSea()) args.unshift(entry);
  const child = spawn(process.execPath, args, {
    detached: true,
    windowsHide: true,
    stdio: ['ignore', log.fd, log.fd],
    env: { ...process.env, ONECREW_LITE_DATA_DIR: paths.dataDir },
  });
  try {
    await once(child, 'spawn');
  } finally {
    await log.close();
  }
  child.unref();
  for (let attempt = 0; attempt < 150; attempt++) {
    if (child.exitCode !== null) throw new Error(`Startup failed; see ${paths.logFile}`);
    let current;
    try {
      current = await readRuntime();
    } catch {
      current = undefined;
    }
    if (current && current.pid === child.pid) {
      try {
        await verifyRuntime(current);
        if (!quiet) console.log(`started (${current.pid}) at ${current.url}`);
        return;
      } catch {
        /* Wait for the child to become ready. */
      }
    }
    await delay(200);
  }
  throw new Error(`Startup timed out; inspect ${paths.logFile} before retrying`);
}

async function stop() {
  let current;
  try {
    current = await readRuntime();
  } catch {
    const lock = await probeRuntimeLock();
    if (lock !== 'locked') {
      console.log('onecrew_lite stopped');
      return;
    }
    throw new Error('onecrew_lite is unavailable; refusing to stop an unverified instance');
  }
  if (!current) {
    if ((await probeRuntimeLock()) === 'locked')
      throw new Error('onecrew_lite is starting; refusing to stop an unverified instance');
    console.log('onecrew_lite stopped');
    return;
  }
  try {
    await verifyRuntime(current);
  } catch {
    if ((await probeRuntimeLock()) !== 'locked') {
      console.log('onecrew_lite stopped');
      return;
    }
    throw new Error('onecrew_lite is unavailable; refusing to stop an unverified instance');
  }
  const response = await fetch(`${current.url}/_local/shutdown`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${current.control_token}` },
    signal: AbortSignal.timeout(2000),
  });
  if (!response.ok) throw new Error('onecrew_lite rejected shutdown');
  for (let attempt = 0; attempt < 100; attempt++) {
    if ((await probeRuntimeLock()) !== 'locked') {
      console.log('onecrew_lite stopped');
      return;
    }
    await delay(100);
  }
  throw new Error(
    `Shutdown timed out; inspect ${paths.logFile}. No forced termination was attempted.`,
  );
}

try {
  if (process.exitCode !== undefined) {
    // parseArgs already reported a concise error.
  } else if (values.version) {
    if (positionals.length) throw new Error('--version does not accept a command');
    console.log(APP_VERSION);
  } else if (!command) {
    if (positionals.length > 0) throw new Error('Expected a command');
    printHelp();
  } else if (command === 'help') {
    if (positionals.length > 2) throw new Error('Expected one help topic');
    printHelp(positionals[1]);
  } else if (values.help) {
    if (!isCommand(command)) throw new Error(`Unknown command: ${command}`);
    if (positionals.length !== 1) throw new Error('Expected exactly one command');
    printHelp(command);
  } else if (positionals.length !== 1) {
    throw new Error('Expected exactly one command');
  } else if (!isCommand(command)) {
    throw new Error(`Unknown command: ${command}`);
  } else if (command === 'configure-plugin') {
    await configurePlugin();
  } else {
    if (values['plugin-dir']) throw new Error('--plugin-dir is only valid for configure-plugin');
    if (values['data-dir']) process.env.ONECREW_LITE_DATA_DIR = values['data-dir'];
    if (values.port) process.env.PORT = values.port;
    paths = runtimePaths();
    if (command === 'serve') {
      await import('./index.js');
    } else if (command === 'mcp') {
      await import('./mcp/stdio.js');
    } else if (command === 'web') {
      await start(true);
      const current = await readRuntime();
      if (!current) throw new Error('Library service unavailable after startup');
      await verifyRuntime(current);
      console.log(JSON.stringify({ url: `${current.url}/library` }));
    } else if (command === 'start') await start();
    else if (command === 'stop') await stop();
    else if (command === 'restart') {
      await stop();
      await start();
    } else if (command === 'status') {
      let current;
      try {
        current = await readRuntime();
      } catch {
        current = undefined;
      }
      let running = false;
      if (current) {
        try {
          await verifyRuntime(current);
          console.log(`running (${current.pid}) at ${current.url}\ndata: ${paths.dataDir}`);
          running = true;
        } catch {
          /* The OS lock decides whether this is stopped or unavailable. */
        }
      }
      if (!running) {
        const lock = await probeRuntimeLock();
        if (lock === 'locked') throw new Error('onecrew_lite is unavailable; inspect the profile');
        console.log(`onecrew_lite stopped (${paths.dataDir})`);
      }
    }
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
