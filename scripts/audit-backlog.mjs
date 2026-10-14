import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Read-only audit of the documented Backlog.md contract; all mutations use CLI.
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const directory = resolve(root, 'backlog/tasks');
const tasks = readdirSync(directory).filter(name => name.endsWith('.md')).map(name => {
  const content = readFileSync(resolve(directory, name), 'utf8');
  const header = content.split('---')[1] ?? '';
  const id = header.match(/^id: (TASK-\d+)$/m)?.[1];
  const dependencies = (header.match(/^dependencies:\n((?:  - .+\n)*)/m)?.[1] ?? '').match(/TASK-\d+/g) ?? [];
  return { id, dependencies, content, name };
});
const byId = new Map(tasks.map(task => [task.id, task]));
const errors = [];
for (const task of tasks) {
  if (!task.id) errors.push(`Missing ID: ${task.name}`);
  for (const required of ['## Testing steps', 'Code/provider tests:', 'Real-binary TUI E2E', 'cargo test --test tui_e2e', 'cargo test --all-targets', 'mouse', 'keyboard', '<!-- AC:BEGIN -->', '<!-- DOD:BEGIN -->']) {
    if (!task.content.includes(required)) errors.push(`${task.id} lacks ${required}`);
  }
  for (const dependency of task.dependencies) {
    if (!byId.has(dependency)) errors.push(`${task.id} references unknown ${dependency}`);
  }
  for (const match of task.content.matchAll(/^  - (backlog\/docs\/.+\.md)$/gm)) {
    if (!existsSync(resolve(root, match[1]))) errors.push(`${task.id} references missing document ${match[1]}`);
  }
}
const active = new Set();
const finished = new Set();
function visit(id) {
  if (active.has(id)) { errors.push(`Dependency cycle at ${id}`); return; }
  if (finished.has(id)) return;
  active.add(id);
  for (const dependency of byId.get(id)?.dependencies ?? []) visit(dependency);
  active.delete(id);
  finished.add(id);
}
for (const task of tasks) visit(task.id);
console.log(JSON.stringify({ tasks: tasks.length, valid: errors.length === 0, errors }, null, 2));
if (errors.length) process.exitCode = 1;
