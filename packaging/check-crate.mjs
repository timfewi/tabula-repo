import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const allowed = /^(?:\.cargo_vcs_info\.json|Cargo\.(?:toml(?:\.orig)?|lock)|LICENSE|README\.md|THIRD_PARTY_LICENSES|docs\/reference\.md|src\/[a-z_]+\.rs|src\/help\/[a-z-]+\.txt|tests\/[a-z_]+\.rs|tests\/reference\/[a-z-]+\.(?:json|sha256))$/;
export function checkCrateMembers(listing) {
  const names = listing.trim().split(/\r?\n/);
  for (const name of names) {
    const slash = name.indexOf('/');
    assert.equal(name.slice(0, slash), 'tabula-repo-0.1.0');
    assert(allowed.test(name.slice(slash + 1)), `Unexpected crate member: ${name}`);
  }
  assert(names.some(n => n.endsWith('/src/main.rs')));
  return names.length;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const p = spawnSync('tar', ['-tzf', process.argv[2]], { encoding: 'utf8' });
  assert.equal(p.status, 0, p.stderr);
  console.log(`Crate allowlist passed: ${checkCrateMembers(p.stdout)} files.`);
}
