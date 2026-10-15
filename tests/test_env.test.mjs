import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { testEnvironment } from '../scripts/test-env.mjs';

test('test_framework_git_maintenance appends without mutating the caller', () => {
  const base = Object.freeze({ PATH: '/fixture/path', OTHER: 'unchanged' });
  const env = testEnvironment(base);
  assert.equal(base.GIT_CONFIG_COUNT, undefined);
  assert.equal(env.PATH, base.PATH);
  assert.equal(env.OTHER, base.OTHER);
  assert.equal(env.GIT_CONFIG_COUNT, '2');
  assert.equal(env.GIT_CONFIG_KEY_0, 'maintenance.autoDetach');
  assert.equal(env.GIT_CONFIG_KEY_1, 'gc.autoDetach');
  assert.equal(env.GIT_CONFIG_VALUE_0, 'false');
  assert.equal(env.GIT_CONFIG_VALUE_1, 'false');
});

test('test_framework_git_maintenance retains existing indexed configuration and parameters', () => {
  const base = Object.freeze({ GIT_CONFIG_COUNT: '2', GIT_CONFIG_KEY_0: 'user.name',
    GIT_CONFIG_VALUE_0: 'Fixture User', GIT_CONFIG_KEY_1: 'maintenance.autoDetach',
    GIT_CONFIG_VALUE_1: 'true', GIT_CONFIG_PARAMETERS: "'color.ui=never'" });
  const env = testEnvironment(base);
  for (const [key, value] of Object.entries(base)) {
    if (key !== 'GIT_CONFIG_COUNT') assert.equal(env[key], value);
  }
  assert.equal(env.GIT_CONFIG_COUNT, '4');
  assert.equal(env.GIT_CONFIG_KEY_2, 'maintenance.autoDetach');
  assert.equal(env.GIT_CONFIG_VALUE_2, 'false');
  assert.equal(env.GIT_CONFIG_KEY_3, 'gc.autoDetach');
  assert.equal(base.GIT_CONFIG_VALUE_1, 'true');
});

test('test_framework_git_maintenance rejects malformed incomplete or oversized configuration', () => {
  for (const count of ['-1', '1.0', ' 1', '01', 'NaN', '4095', '999999999999999999999']) {
    assert.throws(() => testEnvironment({ GIT_CONFIG_COUNT: count }), /Git configuration/);
  }
  for (const extra of [{}, { GIT_CONFIG_KEY_0: 'user.name' },
    { GIT_CONFIG_KEY_0: '', GIT_CONFIG_VALUE_0: 'x' },
    { GIT_CONFIG_KEY_0: 'user.name', GIT_CONFIG_VALUE_0: 'bad\0value' }]) {
    assert.throws(() => testEnvironment({ GIT_CONFIG_COUNT: '1', ...extra }), /Git configuration/);
  }
  assert.equal(testEnvironment({ GIT_CONFIG_COUNT: '' }).GIT_CONFIG_COUNT, '2');
  assert.equal(testEnvironment({ GIT_CONFIG_COUNT: '0' }).GIT_CONFIG_COUNT, '2');
});

test('test_framework_git_maintenance installed Git runs maintenance foreground and leaves config intact', () => {
  const root = mkdtempSync(join(tmpdir(), 'ghtui-maintenance-policy-'));
  try {
    const trace = join(root, 'trace.jsonl');
    const env = testEnvironment({ ...process.env, GIT_CONFIG_COUNT: '1',
      GIT_CONFIG_KEY_0: 'user.name', GIT_CONFIG_VALUE_0: 'Environment Fixture User',
      GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null', GIT_TRACE2_EVENT: trace });
    const git = args => {
      const result = spawnSync('git', ['-C', root, ...args], { env, encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      return result.stdout;
    };
    git(['init', '--initial-branch=main']);
    git(['config', 'user.email', 'fixture@example.invalid']);
    git(['config', 'maintenance.autoDetach', 'true']);
    git(['config', 'gc.autoDetach', 'true']);
    const config = readFileSync(join(root, '.git/config'));
    writeFileSync(join(root, 'file'), 'actual saved bytes\n');
    git(['add', '--', 'file']);
    git(['-c', 'commit.gpgsign=false', 'commit', '-m', 'Actual foreground fixture']);
    assert.equal(git(['log', '-1', '--format=%an']).trim(), 'Environment Fixture User');
    assert.equal(git(['config', '--bool', 'maintenance.autoDetach']).trim(), 'false');
    assert.equal(git(['-c', 'maintenance.autoDetach=true', 'config', '--bool', 'maintenance.autoDetach']).trim(), 'true');
    assert.deepEqual(readFileSync(join(root, '.git/config')), config);
    const maintenance = readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse)
      .filter(event => event.event === 'child_start' && event.argv?.includes('maintenance'));
    assert.equal(maintenance.length, 1);
    assert(maintenance[0].argv.includes('--no-detach'));
    assert(!maintenance[0].argv.includes('--detach'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('test_framework_git_maintenance whole entrypoint refuses hidden selection or ignored overrides', () => {
  const result = spawnSync(process.execPath, ['scripts/test-all.mjs', '--ignored'], { encoding: 'utf8' });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /no filters or ignored-test overrides/);
});
