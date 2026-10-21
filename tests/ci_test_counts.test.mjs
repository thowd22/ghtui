import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checkCounts, parseArgs, parseResults } from '../scripts/ci-test-counts.mjs';

const ok = (passed, filtered = 0, ignored = 0) =>
  `test result: ok. ${passed} passed; 0 failed; ${ignored} ignored; 0 measured; ${filtered} filtered out; finished in 0.01s`;

test('ci_test_counts parses coloured cargo result lines', () => {
  const log = `running 2 tests\n\x1b[32m${ok(2)}\x1b[0m\r\n${ok(0, 9)}\n`;
  assert.deepEqual(parseResults(log).map((r) => [r.passed, r.filteredOut]), [[2, 0], [0, 9]]);
});

test('ci_test_counts refuses logs where nothing ran or a test is ignored', () => {
  assert.throws(() => checkCounts('error: could not compile `ghtui`\n'), /no `test result:` line/);
  assert.throws(() => checkCounts(`${ok(0, 40)}\n${ok(0)}\n`), /only 0 test\(s\) passed/);
  assert.throws(() => checkCounts(`${ok(3, 0, 1)}\n`), /ignored/);
  assert.throws(() => checkCounts(`${ok(10)}\n`, { minPassed: 11 }), /at least 11/);
  assert.deepEqual(checkCounts(`${ok(10)}\n${ok(0)}\n`), { binaries: 2, passed: 10, failed: 0, ignored: 0 });
});

test('ci_test_counts --each refuses a binary whose filter matched nothing', () => {
  assert.throws(() => checkCounts(`${ok(4)}\n${ok(0, 12)}\n`, { each: true }), /1 of 2 test binaries selected zero/);
  const failing = 'test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out';
  assert.equal(checkCounts(`${ok(4)}\n${failing}\n`, { each: true }).failed, 1);
});

test('ci_test_counts argument parsing needs exactly one source', () => {
  assert.throws(() => parseArgs([]), /exactly one/);
  assert.throws(() => parseArgs(['--log', 'a', '--', 'cargo']), /exactly one/);
  assert.throws(() => parseArgs(['--min-passed', '0', '--log', 'a']), /positive integer/);
  assert.deepEqual(parseArgs(['--each', '--', 'cargo', 'test', '--', 'audit_']).command, ['cargo', 'test', '--', 'audit_']);
});

test('ci_test_counts command mode fails on zero selected tests and on command failure', () => {
  const dir = mkdtempSync(join(tmpdir(), 'ci-test-counts-'));
  try {
    const script = new URL('../scripts/ci-test-counts.mjs', import.meta.url).pathname;
    const log = join(dir, 'zero.log');
    writeFileSync(log, `${ok(0, 5)}\n`);
    const zero = spawnSync('node', [script, '--log', log], { encoding: 'utf8' });
    assert.equal(zero.status, 1);
    assert.match(zero.stderr, /only 0 test\(s\) passed/);
    // The child sees the test-only Git maintenance policy.
    const printer = `console.log(process.env.GIT_CONFIG_KEY_0 + ' ' + process.env.GIT_CONFIG_VALUE_0); console.log(${JSON.stringify(ok(3))})`;
    const passing = spawnSync('node', [script, '--each', '--', 'node', '-e', printer], { encoding: 'utf8', env: { PATH: process.env.PATH } });
    assert.equal(passing.status, 0, passing.stderr);
    assert.match(passing.stdout, /maintenance\.autoDetach false/);
    assert.match(passing.stdout, /1 binaries, 3 passed/);
    const failing = spawnSync('node', [script, '--', 'node', '-e', `console.log(${JSON.stringify(ok(3))}); process.exit(101)`], { encoding: 'utf8' });
    assert.equal(failing.status, 1);
    assert.match(failing.stderr, /exited with status 101/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
