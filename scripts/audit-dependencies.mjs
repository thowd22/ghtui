#!/usr/bin/env node
// Source integrity is checked without contacting a registry or remote.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
const assert = (condition, message) => { if (!condition) throw new Error(message); };

export function safePath(root, relative) {
  assert(typeof relative === "string" && !path.isAbsolute(relative), `absolute source path: ${relative}`);
  assert(!relative.split(/[\\/]/).includes(".."), `escaping source path: ${relative}`);
  const candidate = path.resolve(root, relative);
  const real = fs.realpathSync(candidate);
  assert(real.startsWith(`${fs.realpathSync(root)}${path.sep}`), `source outside project: ${relative}`);
  return real;
}

export function verifyFileChecksum(root, relative, expected) {
  assert(/^[0-9a-f]{64}$/.test(expected), "invalid SHA-256 checksum");
  assert(sha256(fs.readFileSync(safePath(root, relative))) === expected, `file checksum mismatch: ${relative}`);
}

function inventory(root, relative, result = {}) {
  const directory = safePath(root, relative);
  for (const item of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const next = `${relative}/${item.name}`;
    assert(item.name !== ".git", `embedded Git metadata: ${next}`);
    if (item.isSymbolicLink()) {
      const linked = safePath(root, next);
      const metadata = fs.statSync(linked);
      assert(metadata.isFile() || metadata.isDirectory(), `unsupported source link: ${next}`);
      result[next] = sha256(Buffer.concat([Buffer.from(`symlink:${fs.readlinkSync(path.join(directory, item.name))}\0`), metadata.isFile() ? fs.readFileSync(linked) : Buffer.from("directory")]));
    } else if (item.isDirectory()) inventory(root, next, result);
    else if (item.isFile()) result[next] = sha256(fs.readFileSync(safePath(root, next)));
  }
  return result;
}

export function audit(root, { refresh = false } = {}) {
  const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const sources = readJson(path.join(root, "vendor/upstream-sources.json"));
  assert(sources.schema === 1 && sources.sources.length > 0, "no recorded upstream sources");
  const recorded = new Set();
  for (const source of sources.sources) {
    assert(/^vendor\/upstream\/[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(source.directory), "invalid upstream directory");
    assert(/^[0-9a-f]{40}$/.test(source.revision), `missing exact Git pin: ${source.directory}`);
    assert(/^https:\/\//.test(source.repository), `missing upstream origin: ${source.directory}`);
    const directory = safePath(root, source.directory);
    const declared = typeof source.license_declaration === "string"
      && fs.readFileSync(path.join(directory, "Cargo.toml"), "utf8").includes(`license = "${source.license_declaration}"`);
    const license = fs.readdirSync(directory).some((name) => /(LICENSE|COPYING)/i.test(name))
      || declared
      || (fs.existsSync(path.join(directory, "AUTHORS")) && fs.readFileSync(path.join(directory, "AUTHORS"), "utf8").startsWith("LICENSE:"));
    assert(license, `missing license: ${source.directory}`);
    assert(source.modified_files.every((name) => name.endsWith("Cargo.toml")), `unrecorded runtime-source adaptation: ${source.directory}`);
    recorded.add(source.directory);
  }
  let patchCount = 0;
  for (const match of cargo.matchAll(/path\s*=\s*"([^"]+)"/g)) {
    const relative = match[1];
    assert(relative.startsWith("vendor/upstream/"), `nonportable dependency: ${relative}`);
    safePath(root, relative);
    assert(recorded.has(relative.split("/").slice(0, 3).join("/")), `unrecorded dependency pin: ${relative}`);
    patchCount++;
  }
  assert(patchCount > 0, "no actual source patches checked");
  const actual = {};
  for (const source of sources.sources) inventory(root, source.directory, actual);
  const checksumFile = path.join(root, "vendor/upstream-checksums.json");
  if (refresh) fs.writeFileSync(checksumFile, `${JSON.stringify({ schema: 1, files: actual }, null, 2)}\n`);
  const frozen = readJson(checksumFile);
  assert(JSON.stringify(Object.keys(actual).sort()) === JSON.stringify(Object.keys(frozen.files).sort()), "upstream file inventory changed");
  for (const [name, hash] of Object.entries(actual)) assert(frozen.files[name] === hash, `upstream checksum mismatch: ${name}`);

  const lock = fs.readFileSync(path.join(root, "Cargo.lock"), "utf8");
  let registryCount = 0;
  for (const block of lock.split("[[package]]").slice(1)) {
    if (!block.includes('source = "registry+')) continue;
    const name = block.match(/^name = "([^"]+)"/m)?.[1];
    const version = block.match(/^version = "([^"]+)"/m)?.[1];
    const checksum = block.match(/^checksum = "([^"]+)"/m)?.[1];
    assert(name && version && checksum, "incomplete registry lock entry");
    const relative = `vendor/crates/${name}-${version}`;
    const metadata = readJson(path.join(safePath(root, relative), ".cargo-checksum.json"));
    assert(metadata.package === checksum, `registry package checksum differs from lock: ${name}`);
    for (const [file, hash] of Object.entries(metadata.files)) {
      verifyFileChecksum(root, `${relative}/${file}`, hash);
    }
    registryCount++;
  }
  assert(registryCount > 0, "no registry packages checked");
  return { upstreamSources: sources.sources.length, upstreamFiles: Object.keys(actual).length, patches: patchCount, registryPackages: registryCount };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
    console.log(JSON.stringify(audit(root, { refresh: process.argv.includes("--refresh-upstream-checksums") })));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
