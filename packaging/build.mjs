// Package an explicitly selected, already-built native binary. Never downloads.
import { readFileSync, mkdirSync, copyFileSync, writeFileSync, chmodSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const version = JSON.parse(readFileSync(join(root, 'package.json'))).version;
const platforms = {
  'x86_64-unknown-linux-musl': ['linux', 'x64'],
  'aarch64-unknown-linux-musl': ['linux', 'arm64'],
  'x86_64-apple-darwin': ['darwin', 'x64'],
  'aarch64-apple-darwin': ['darwin', 'arm64'],
  'x86_64-pc-windows-msvc': ['win32', 'x64'],
};
const [target, binary, destination] = process.argv.slice(2);
if (!platforms[target] || !binary || !destination || process.argv.length !== 5) {
  console.error('Usage: node packaging/build.mjs TARGET BINARY NEW_OUTPUT_DIRECTORY');
  process.exit(2);
}
const output = resolve(destination);
if (existsSync(output)) throw new Error('output directory already exists');
const [os, cpu] = platforms[target];
if (process.platform !== os || process.arch !== cpu) throw new Error('package on the matching native runner');
const executable = os === 'win32' ? 'tabula.exe' : 'tabula';
const probe = spawnSync(resolve(binary), ['--version'], { encoding: 'utf8' });
if (probe.status !== 0 || probe.stdout !== `Tabula ${version}\n`) throw new Error('binary version check failed');
const temporary = mkdtempSync(join(tmpdir(), 'tabula-package-'));
function run(program, args, cwd) {
  const p = spawnSync(program, args, { cwd, stdio: 'inherit' });
  if (p.status !== 0) throw new Error(`${program} failed`);
}
try {
  mkdirSync(output);
  const basename = `tabula-${version}-${target}`;
  const stage = join(temporary, basename);
  mkdirSync(stage);
  copyFileSync(resolve(binary), join(stage, executable));
  chmodSync(join(stage, executable), 0o755);
  for (const file of ['LICENSE', 'README.md', 'THIRD_PARTY_LICENSES']) copyFileSync(join(root, file), join(stage, file));
  copyFileSync(join(root, 'docs/reference.md'), join(stage, 'REFERENCE.md'));
  writeFileSync(join(stage, 'VERSION'), `${version}\n`);
  const archive = join(output, basename + (os === 'win32' ? '.zip' : '.tar.gz'));
  if (os === 'win32') {
    // Parameters reach PowerShell through environment, never command interpolation.
    const p = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Compress-Archive -LiteralPath $env:TABULA_STAGE -DestinationPath $env:TABULA_ARCHIVE'], {
      env: { ...process.env, TABULA_STAGE: stage, TABULA_ARCHIVE: archive }, stdio: 'inherit',
    });
    if (p.status !== 0) throw new Error('archive failed');
  } else run('tar', ['-czf', archive, '-C', temporary, basename], temporary);
  writeFileSync(archive + '.sha256', `${createHash('sha256').update(readFileSync(archive)).digest('hex')}  ${basename}${os === 'win32' ? '.zip' : '.tar.gz'}\n`);
  const pkg = join(output, 'platform');
  mkdirSync(pkg);
  copyFileSync(join(stage, executable), join(pkg, executable));
  chmodSync(join(pkg, executable), 0o755);
  for (const file of ['README.md', 'LICENSE', 'THIRD_PARTY_LICENSES']) copyFileSync(join(stage, file), join(pkg, file));
  writeFileSync(join(pkg, 'package.json'), JSON.stringify({
    name: `tabula-repo-${target}`, version, license: 'MIT', os: [os], cpu: [cpu],
    files: [executable, 'LICENSE', 'README.md', 'THIRD_PARTY_LICENSES'],
    ...(os === 'linux' ? { libc: ['musl', 'glibc'] } : {}),
  }, null, 2) + '\n');
} finally { rmSync(temporary, { recursive: true, force: true }); }
