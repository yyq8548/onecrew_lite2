#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { constants } = require('node:os');

const manifest = require('../package.json');
const target = `${process.platform}-${process.arch}`;
const platformPackage = `${manifest.name}-${target}`;
if (!Object.hasOwn(manifest.optionalDependencies, platformPackage)) {
  throw new Error(`OneCrew Lite does not support ${target}. Use Apple Silicon macOS or Windows x64 with matching Node.js.`);
}
const platformVersion = manifest.optionalDependencies[platformPackage];
if (typeof platformVersion !== 'string' || platformVersion.length === 0) {
  throw new Error(`OneCrew Lite requires an exact version for ${platformPackage} in optionalDependencies.`);
}
const platformManifestPath = require.resolve(`${platformPackage}/package.json`);
const platformManifest = require(platformManifestPath);
if (platformManifest.version !== platformVersion) {
  throw new Error(`OneCrew Lite requires ${platformPackage}@${platformVersion}. Reinstall with --include=optional.`);
}
const binary = path.join(path.dirname(platformManifestPath), process.platform === 'win32' ? 'onecrew_lite.exe' : 'onecrew_lite');
if (!fs.statSync(binary).isFile()) throw new Error(`OneCrew Lite binary is missing: ${binary}`);

const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--print-binary-path') {
  console.log(binary);
} else {
  const child = spawn(binary, args, { stdio: 'inherit', windowsHide: true });
  const handlers = new Map();
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    const handler = () => child.kill(signal);
    handlers.set(signal, handler);
    process.on(signal, handler);
  }
  child.once('error', error => {
    console.error(`OneCrew Lite could not start: ${error.message}`);
    process.exitCode = 1;
  });
  child.once('exit', (code, signal) => {
    for (const [name, handler] of handlers) process.removeListener(name, handler);
    process.exitCode = code ?? 128 + (constants.signals[signal] ?? 1);
  });
}
