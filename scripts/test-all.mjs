import { spawn, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { testEnvironment } from './test-env.mjs';

// Execute the required unfiltered Cargo gate with test-only child policy.
// Resource scopes/locking belong to the coordinating development host, not
// this portable macOS/Linux entrypoint or the installed application.
//
// CI-only: GHTUI_CI_RETRY_FAILED=1 reruns each failed test once, alone, so a
// timing flake on a saturated hosted runner does not fail the run. A test that
// fails again still fails it, every retried test is listed, and more than
// MAX_RETRIES failures are treated as real breakage without retrying.
const MAX_RETRIES = 5;

function runGate(root, retry) {
  return new Promise((resolvePromise, reject) => {
    const args = ['test', '--locked', '--all-targets', '--no-fail-fast'];
    const child = spawn('cargo', args, {
      cwd: root,
      env: testEnvironment(),
      stdio: ['inherit', retry ? 'pipe' : 'inherit', retry ? 'pipe' : 'inherit'],
    });
    const failures = [];
    // Retry only when the named failures account for every failing binary:
    // a crashed binary (no result line) or a build error must still fail.
    const tally = { reported: 0, failedResults: 0, failedBinaries: 0 };
    const scan = (stream, sink) => {
      let pending = '';
      stream.on('data', (chunk) => {
        sink.write(chunk);
        pending += chunk.toString('utf8');
        const lines = pending.split('\n');
        pending = lines.pop();
        for (const line of lines) {
          const failed = /^test (\S+) \.\.\. FAILED$/.exec(line);
          if (failed) failures.push(failed[1]);
          const result = /^test result: FAILED\. \d+ passed; (\d+) failed/.exec(line);
          if (result) {
            tally.reported += Number(result[1]);
            tally.failedResults += 1;
          }
          if (/^error: test failed, to rerun pass/.test(line)) tally.failedBinaries += 1;
        }
      });
    };
    if (retry) {
      scan(child.stdout, process.stdout);
      scan(child.stderr, process.stderr);
    }
    child.on('error', reject);
    child.on('close', (status, signal) => {
      if (signal) reject(new Error(`Cargo verification ended with signal ${signal}`));
      else resolvePromise({ status: status ?? 1, failures, tally });
    });
  });
}

if (process.argv.length !== 2) {
  console.error('Usage: node scripts/test-all.mjs (no filters or ignored-test overrides)');
  process.exitCode = 1;
} else {
  try {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
    const retry = process.env.GHTUI_CI_RETRY_FAILED === '1';
    const { status, failures, tally } = await runGate(root, retry);
    process.exitCode = status;
    const retriable =
      retry && status !== 0 && failures.length > 0 && failures.length <= MAX_RETRIES &&
      tally.reported === failures.length && tally.failedBinaries === tally.failedResults;
    if (retriable) {
      let stillFailing = 0;
      for (const name of failures) {
        console.log(`\n=== Retrying ${name} once on its own`);
        // Every test binary runs only this exact name (0 tests where absent).
        const rerun = spawnSync(
          'cargo',
          ['test', '--locked', '--all-targets', '--', '--exact', name],
          { cwd: root, env: testEnvironment(), stdio: 'inherit' },
        );
        if (rerun.status !== 0) stillFailing += 1;
      }
      console.log(`\n=== Flaky tests that failed once and were retried (${failures.length}):`);
      for (const name of failures) console.log(`  ${name}`);
      process.exitCode = stillFailing === 0 ? 0 : 1;
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
