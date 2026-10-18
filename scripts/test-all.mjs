import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { testEnvironment } from './test-env.mjs';

// Execute the required unfiltered Cargo gate with test-only child policy.
// Resource scopes/locking belong to the coordinating development host, not
// this portable macOS/Linux entrypoint or the installed application.
if (process.argv.length !== 2) {
  console.error('Usage: node scripts/test-all.mjs (no filters or ignored-test overrides)');
  process.exitCode = 1;
} else {
  try {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
    const result = spawnSync('cargo', ['test', '--locked', '--all-targets', '--no-fail-fast'], {
      cwd: root,
      env: testEnvironment(),
      stdio: 'inherit',
    });
    if (result.error) throw result.error;
    if (result.signal) throw new Error(`Cargo verification ended with signal ${result.signal}`);
    process.exitCode = result.status ?? 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
