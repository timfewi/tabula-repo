import test from 'node:test';
import assert from 'node:assert/strict';
import { checkCrateMembers } from './check-crate.mjs';

const root = 'tabula-repo-0.1.0';
const members = ['.cargo_vcs_info.json', 'Cargo.toml', 'src/main.rs'];
for (const [name, ending] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`crate allowlist accepts ${name} tar output`, () => {
    const listing = members.map(member => `${root}/${member}`).join(ending) + ending;
    assert.equal(checkCrateMembers(listing), members.length);
  });
  test(`crate allowlist still rejects unsafe ${name} tar output`, () => {
    for (const member of ['.env', '../src/main.rs', '/src/main.rs', 'src/main.rs/extra', 'src/ma\rin.rs']) {
      assert.throws(() => checkCrateMembers(`${root}/${member}${ending}${root}/src/main.rs${ending}`));
    }
    assert.throws(() => checkCrateMembers(`other/src/main.rs${ending}`));
    assert.throws(() => checkCrateMembers(`${root}/Cargo.toml${ending}`));
  });
}
