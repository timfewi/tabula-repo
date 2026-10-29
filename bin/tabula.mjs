#!/usr/bin/env node
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
const require = createRequire(import.meta.url);
const targets = {
  'linux-x64': 'x86_64-unknown-linux-musl',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'win32-x64': 'x86_64-pc-windows-msvc',
};
const target = targets[`${process.platform}-${process.arch}`];
try {
  if (!target) throw new Error('unsupported platform');
  const manifest = require(`tabula-repo-${target}/package.json`);
  if (manifest.version !== '0.1.0') throw new Error('platform package version mismatch');
  const binary = require.resolve(`tabula-repo-${target}/tabula${process.platform === 'win32' ? '.exe' : ''}`);
  const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit' });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
  child.on('error', () => {
    console.error('tabula: could not start the platform binary');
    process.exitCode = 2;
  });
  child.on('exit', (code, signal) => {
    process.exitCode = code ?? (signal === 'SIGINT' ? 130 : signal === 'SIGTERM' ? 143 : 2);
  });
} catch {
  console.error('tabula: matching platform package 0.1.0 is unavailable; install with optional dependencies enabled on a supported platform');
  process.exitCode = 2;
}
