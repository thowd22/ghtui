import { readdirSync, readFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export function selectedNames(listing, key) {
  return listing.split('\n').flatMap(line => {
    const name = line.match(/^(.+): test$/)?.[1];
    return name?.includes(key) ? [name] : [];
  });
}

export function verifyCoverage(codeCount, tuiCount, journeys) {
  if (codeCount < 1) throw new Error('The task selected zero code/provider tests');
  if (tuiCount < 1) throw new Error('The task selected zero real-binary PTY tests');
  const completed = journeys.filter(journey => journey.exited && !journey.panicking);
  if (!completed.some(journey => journey.keyboard)) throw new Error('Missing actual keyboard input coverage');
  if (!completed.some(journey => journey.mouse)) throw new Error('Missing actual mouse input coverage');
  return { code: codeCount, pty: tuiCount, keyboard: true, mouse: true };
}

export function verifyExecuted(output, expected) {
  const result = output.match(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/);
  if (!result || Number(result[1]) !== expected || Number(result[2]) !== 0 || Number(result[3]) !== 0) {
    throw new Error(`Expected ${expected} passing selected tests and zero ignored tests; actual execution did not match`);
  }
}

function cargo(args, options = {}) {
  const result = spawnSync('cargo', ['test', ...args], { cwd: root, encoding: 'utf8', env: { ...process.env, ...options.env }, maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`cargo test ${args.join(' ')} failed\n${result.stdout ?? ''}\n${result.stderr ?? ''}`);
  return result.stdout;
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function runTask(key) {
  if (!key || !/^[a-z][a-z0-9_]*$/.test(key)) throw new Error('Usage: node scripts/test-task.mjs <scenario_key>');
  const targets = [['--lib'], ...readdirSync(resolve(root, 'tests')).filter(file => file.endsWith('.rs') && file !== 'tui_e2e.rs').map(file => ['--test', file.slice(0, -3)])];
  const selected = targets.map(target => ({ target, names: selectedNames(cargo([...target, '--', '--list']), key) })).filter(selection => selection.names.length);
  const codeCount = selected.reduce((count, selection) => count + selection.names.length, 0);
  const tuiNames = selectedNames(cargo(['--test', 'tui_e2e', '--', '--list']), key);
  if (!codeCount || !tuiNames.length) verifyCoverage(codeCount, tuiNames.length, []);
  for (const { target, names } of selected) {
    console.log(`Running ${names.length} code/provider tests in ${target.join(' ')}`);
    const output = cargo([...target, key, '--', '--nocapture']);
    verifyExecuted(output, names.length);
    process.stdout.write(output);
  }
  const artifacts = mkdtempSync(resolve(tmpdir(), 'ghtui-test-coverage-'));
  const coverage = resolve(artifacts, 'journeys.jsonl');
  const output = cargo(['--test', 'tui_e2e', key, '--', '--nocapture'], { env: { GHTUI_TEST_COVERAGE_FILE: coverage } });
  verifyExecuted(output, tuiNames.length);
  process.stdout.write(output);
  const journeys = readFileSync(coverage, 'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
  const result = verifyCoverage(codeCount, tuiNames.length, journeys);
  console.log(JSON.stringify({ key, ...result, artifacts }, null, 2));
  return result;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { runTask(process.argv[2]); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
