import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { testEnvironment } from './test-env.mjs';

// CI guard against silently skipped tests (TASK-69).
//
//   node scripts/ci-test-counts.mjs [--each] [--min-passed N] --log FILE
//       Check the `test result:` lines of an existing cargo test log.
//   node scripts/ci-test-counts.mjs [--each] [--min-passed N] -- cargo test …
//       Run the command with the test-only Git environment (foreground
//       automatic maintenance, see test-env.mjs), stream its output, then check.
//
// The check fails when the log has no `test result:` line, when the selected
// tests passed in total fewer than --min-passed (default 1), when any test is
// ignored, or, with --each, when any single test binary selected zero tests
// (a filter that matched nothing). A failing command still fails the step.
const RESULT =
  /^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out/;

export function parseResults(text) {
  const results = [];
  for (const raw of text.split('\n')) {
    // CI sets CARGO_TERM_COLOR=always, so cargo's own lines carry ANSI.
    const line = raw.replace(/\x1b\[[0-9;]*m/g, '').replace(/\r$/, '').trim();
    const match = RESULT.exec(line);
    if (match) {
      results.push({
        ok: match[1] === 'ok',
        passed: Number(match[2]),
        failed: Number(match[3]),
        ignored: Number(match[4]),
        filteredOut: Number(match[6]),
      });
    }
  }
  return results;
}

export function checkCounts(text, { each = false, minPassed = 1 } = {}) {
  const results = parseResults(text);
  if (results.length === 0) {
    throw new Error('no `test result:` line: no test binary ran');
  }
  const passed = results.reduce((sum, r) => sum + r.passed, 0);
  const failed = results.reduce((sum, r) => sum + r.failed, 0);
  const ignored = results.reduce((sum, r) => sum + r.ignored, 0);
  if (ignored > 0) {
    throw new Error(`${ignored} ignored test(s): required tests must not be ignored`);
  }
  if (each) {
    const empty = results.filter((r) => r.passed + r.failed === 0).length;
    if (empty > 0) {
      throw new Error(`${empty} of ${results.length} test binaries selected zero tests`);
    }
  }
  if (passed < minPassed) {
    throw new Error(`only ${passed} test(s) passed; at least ${minPassed} required`);
  }
  return { binaries: results.length, passed, failed, ignored };
}

export function parseArgs(argv) {
  const options = { each: false, minPassed: 1, log: null, command: null };
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--each') options.each = true;
    else if (arg === '--min-passed') {
      const value = argv[++index] ?? '';
      if (!/^[1-9][0-9]*$/.test(value)) throw new Error('--min-passed needs a positive integer');
      options.minPassed = Number(value);
    } else if (arg === '--log') {
      options.log = argv[++index];
      if (!options.log) throw new Error('--log needs a file');
    } else if (arg === '--') {
      options.command = argv.slice(index + 1);
      break;
    } else throw new Error(`unknown argument ${arg}`);
  }
  if ((options.log === null) === (options.command === null || options.command.length === 0)) {
    throw new Error('give exactly one of --log FILE or -- COMMAND…');
  }
  return options;
}

function runCommand([program, ...args]) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(program, args, {
      env: testEnvironment(),
      stdio: ['inherit', 'pipe', 'pipe'],
    });
    const chunks = [];
    child.stdout.on('data', (chunk) => {
      process.stdout.write(chunk);
      chunks.push(chunk);
    });
    child.stderr.on('data', (chunk) => {
      process.stderr.write(chunk);
      chunks.push(chunk);
    });
    child.on('error', reject);
    child.on('close', (status, signal) => {
      resolvePromise({ status: signal ? 1 : status ?? 1, text: Buffer.concat(chunks).toString('utf8') });
    });
  });
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  let text;
  let status = 0;
  if (options.log !== null) {
    text = readFileSync(options.log, 'utf8');
  } else {
    ({ status, text } = await runCommand(options.command));
  }
  const counts = checkCounts(text, options);
  console.log(
    `ci-test-counts: ${counts.binaries} binaries, ${counts.passed} passed, ` +
      `${counts.failed} failed, ${counts.ignored} ignored`,
  );
  if (status !== 0) throw new Error(`command exited with status ${status}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    await main();
  } catch (error) {
    console.error(`ci-test-counts: ${error.message}`);
    process.exitCode = 1;
  }
}
