// Test-child policy only. Never change process.env or repository/user config.
// A Linux PTY test process is a subreaper: detached Git maintenance otherwise
// escapes the PTY session and accumulates adopted zombies until suite exit.
export function testEnvironment(base = process.env) {
  const env = { ...base };
  const raw = env.GIT_CONFIG_COUNT ?? '';
  if (typeof raw !== 'string' || (raw !== '' && !/^(0|[1-9][0-9]*)$/.test(raw))) {
    throw new Error('Invalid indexed Git configuration count; no test child launched');
  }
  const count = raw === '' ? 0 : Number(raw);
  if (!Number.isSafeInteger(count) || count > 4094) {
    throw new Error('Indexed Git configuration exceeds test-child bound');
  }
  for (let index = 0; index < count; index++) {
    for (const field of ['KEY', 'VALUE']) {
      const value = env[`GIT_CONFIG_${field}_${index}`];
      if (typeof value !== 'string' || value.includes('\0') || (field === 'KEY' && !value)) {
        throw new Error('Incomplete indexed Git configuration; no test child launched');
      }
    }
  }
  for (const [offset, key] of ['maintenance.autoDetach', 'gc.autoDetach'].entries()) {
    env[`GIT_CONFIG_KEY_${count + offset}`] = key;
    env[`GIT_CONFIG_VALUE_${count + offset}`] = 'false';
  }
  env.GIT_CONFIG_COUNT = String(count + 2);
  return env;
}
