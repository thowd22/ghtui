import test from 'node:test';
import assert from 'node:assert/strict';
import { selectedNames, verifyCoverage, verifyExecuted } from '../scripts/test-task.mjs';

test('test_framework scenario filters count actual test names', () => {
  assert.deepEqual(selectedNames('module::test_framework_contract: test\nmodule::other: test\n', 'test_framework'), ['module::test_framework_contract']);
  assert.deepEqual(selectedNames('running 0 tests\n', 'test_framework'), []);
});

test('test_framework requires executed test counts and rejects ignored tests', () => {
  verifyExecuted('test result: ok. 2 passed; 0 failed; 0 ignored;', 2);
  assert.throws(() => verifyExecuted('test result: ok. 0 passed; 0 failed; 0 ignored;', 2), /did not match/);
  assert.throws(() => verifyExecuted('test result: ok. 1 passed; 0 failed; 1 ignored;', 2), /did not match/);
});

test('test_framework rejects zero tests and missing terminal input paths', () => {
  assert.throws(() => verifyCoverage(0, 2, []), /zero code/);
  assert.throws(() => verifyCoverage(2, 0, []), /zero real-binary/);
  assert.throws(() => verifyCoverage(2, 2, [{ exited: true, mouse: true }]), /keyboard/);
  assert.throws(() => verifyCoverage(2, 2, [{ exited: true, keyboard: true }]), /mouse/);
  assert.throws(() => verifyCoverage(2, 2, [{ exited: false, keyboard: true, mouse: true }]), /keyboard/);
  assert.deepEqual(verifyCoverage(2, 2, [{ exited: true, keyboard: true }, { exited: true, mouse: true }]), { code: 2, pty: 2, keyboard: true, mouse: true });
});
