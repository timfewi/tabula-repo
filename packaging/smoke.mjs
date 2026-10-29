// Run reviewed local artifacts outside the checkout. No registry or global install.
import { mkdtempSync, rmSync, mkdirSync, readFileSync, writeFileSync, existsSync, readdirSync, symlinkSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const packages = resolve(process.argv[2]);
const temporary = mkdtempSync(join(tmpdir(), 'tabula-smoke-'));
const windows = process.platform === 'win32';
function run(program, args, options = {}, expected = 0) {
  const p = spawnSync(program, args, { cwd: temporary, encoding: 'utf8', ...options });
  assert.equal(p.status, expected, `${program}: ${p.error ?? ''}\n${p.stdout}\n${p.stderr}`);
  return p;
}
function npmPack(directory) {
  // npm.cmd must use cmd.exe on Windows, with fixed argument layout.
  const args = ['pack', directory, '--ignore-scripts', '--json', '--pack-destination', temporary];
  const p = windows ? run('cmd.exe', ['/d', '/s', '/c', 'npm', ...args]) : run('npm', args);
  const info = JSON.parse(p.stdout)[0];
  assert(info.files.every(f => /^(?:package\.json|README\.md|LICENSE|THIRD_PARTY_LICENSES|docs\/reference\.md|bin\/tabula\.mjs|tabula(?:\.exe)?)$/.test(f.path)), 'Unexpected npm package member');
  return join(temporary, info.filename);
}
try {
  const platform = JSON.parse(readFileSync(join(packages, 'platform/package.json')));
  const mainArchive = npmPack(root);
  const nativeArchive = npmPack(join(packages, 'platform'));
  const app = join(temporary, 'app'); mkdirSync(app);
  run('tar', ['-xf', mainArchive, '-C', app]);
  const module = join(app, 'package', 'node_modules', platform.name);
  mkdirSync(module, { recursive: true });
  run('tar', ['-xf', nativeArchive, '--strip-components=1', '-C', module]);
  const launcher = join(app, 'package/bin/tabula.mjs');
  const project = join(temporary, 'project'); mkdirSync(project);
  writeFileSync(join(project, 'README.md'), '# Example\n');
  for (const runtime of [process.execPath, 'bun']) {
    assert.equal(run(runtime, [launcher, '--version']).stdout, 'Tabula 0.1.0\n');
  }
  const binary = join(module, windows ? 'tabula.exe' : 'tabula');
  const isolated = { env: { ...process.env, PATH: '', PYTHONPATH: '', PYTHONHOME: '' }, cwd: project };
  run(binary, ['init', '.', '--purpose', 'Package test', '--json'], isolated);
  assert.equal(JSON.parse(run(binary, ['check', '.', '--json'], isolated).stdout).status, 'current');
  writeFileSync(join(project, 'README.md'), '# Changed\n');
  assert.equal(JSON.parse(run(binary, ['check', '.', '--json'], isolated, 1).stdout).status, 'stale');
  const manifestPath = join(module, 'package.json');
  writeFileSync(manifestPath, JSON.stringify({ ...platform, version: '0.0.0' }));
  run(process.execPath, [launcher, '--version'], {}, 2);
  writeFileSync(manifestPath, JSON.stringify(platform));
  const archiveName = readdirSync(packages).find(name => name.endsWith(windows ? '.zip' : '.tar.gz'));
  assert(archiveName);
  const archive = join(packages, archiveName);
  const hash = createHash('sha256').update(readFileSync(archive)).digest('hex');
  const prefix = join(temporary, 'installed');
  if (windows) {
    const installer = join(root, 'install.ps1');
    const args = ['-NoProfile', '-NonInteractive', '-File', installer, '-Version', '0.1.0', '-Archive', archive, '-Sha256', hash, '-Prefix', prefix];
    const badChecksum = [...args]; badChecksum[badChecksum.indexOf('-Sha256') + 1] = '0'.repeat(64);
    run('powershell.exe', badChecksum, {}, 2);
    assert(!existsSync(prefix));
    const linkedPrefix = join(temporary, 'junction');
    run('cmd.exe', ['/d', '/c', 'mklink', '/J', linkedPrefix, project]);
    const linkedArgs = [...args]; linkedArgs[linkedArgs.indexOf('-Prefix') + 1] = linkedPrefix;
    run('powershell.exe', linkedArgs, {}, 2);
    run('powershell.exe', args);
    run('powershell.exe', args, {}, 2);
    assert.equal(run(join(prefix, 'tabula.exe'), ['--version'], isolated).stdout, 'Tabula 0.1.0\n');
  } else {
    const installer = join(root, 'install.sh');
    const malformedStage = join(temporary, 'malformed'); mkdirSync(malformedStage);
    const top = archiveName.slice(0, -7);
    const badTop = join(malformedStage, top); mkdirSync(badTop);
    for (const name of ['LICENSE', 'README.md', 'REFERENCE.md', 'VERSION', 'THIRD_PARTY_LICENSES']) writeFileSync(join(badTop, name), name === 'VERSION' ? '0.1.0\n' : 'fixture');
    symlinkSync(binary, join(badTop, 'tabula'));
    const badArchive = join(temporary, 'links.tar.gz');
    run('tar', ['-czf', badArchive, '-C', malformedStage, top]);
    const badHash = createHash('sha256').update(readFileSync(badArchive)).digest('hex');
    run('sh', [installer, '0.1.0', badArchive, badHash, prefix], {}, 2);
    assert(!existsSync(prefix));
    const linkedPrefix = join(temporary, 'linked'); symlinkSync(project, linkedPrefix);
    run('sh', [installer, '0.1.0', archive, hash, linkedPrefix], {}, 2);
    const shim = join(temporary, 'failure-tools'); mkdirSync(shim);
    writeFileSync(join(shim, 'cat'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
    const failedPrefix = join(temporary, 'failed-install');
    run('sh', [installer, '0.1.0', archive, hash, failedPrefix], { env: { ...process.env, PATH: `${shim}:${process.env.PATH}` } }, 2);
    assert(!existsSync(join(failedPrefix, 'bin/tabula')));
    assert(!existsSync(join(failedPrefix, 'lib/tabula/0.1.0')));
    run('sh', [installer, '0.1.0', archive, '0'.repeat(64), prefix], {}, 2);
    assert(!existsSync(prefix));
    run('sh', [installer, '0.1.0', archive, hash, prefix]);
    run('sh', [installer, '0.1.0', archive, hash, prefix], {}, 2);
    assert.equal(run(join(prefix, 'bin/tabula'), ['--version'], isolated).stdout, 'Tabula 0.1.0\n');
  }
  rmSync(module, { recursive: true });
  run(process.execPath, [launcher, '--version'], {}, 2);
  console.log('Native CLI, npm tarballs, Node/Bun, installer, checksum, overwrite and missing/version refusal checks passed.');
} finally { rmSync(temporary, { recursive: true, force: true }); }
