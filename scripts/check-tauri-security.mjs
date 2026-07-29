import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const configPath = path.join(repoRoot, "apps", "desktop", "src-tauri", "tauri.conf.json");
const config = JSON.parse(await readFile(configPath, "utf8"));
const security = config.app?.security ?? {};

const requiredDirectives = [
  "default-src",
  "script-src",
  "style-src",
  "img-src",
  "font-src",
  "connect-src",
  "object-src",
  "base-uri",
  "form-action",
  "frame-ancestors",
];

const production = normalizeCsp(security.csp, "app.security.csp");
const development = normalizeCsp(security.devCsp, "app.security.devCsp");

for (const directive of requiredDirectives) {
  assert(production.has(directive), `production CSP missing ${directive}`);
  assert(development.has(directive), `development CSP missing ${directive}`);
}

assertSource(production, "default-src", "'self'");
assertSource(production, "script-src", "'self'");
assertSource(production, "connect-src", "ipc:");
assertSource(production, "connect-src", "http://ipc.localhost");
assertSource(production, "object-src", "'none'");
assertSource(production, "form-action", "'none'");
assertSource(production, "frame-ancestors", "'none'");

assertSource(development, "connect-src", "ws://127.0.0.1:1420");
assertSource(development, "connect-src", "http://127.0.0.1:1420");
assertSource(development, "connect-src", "ipc:");
assertSource(development, "connect-src", "http://ipc.localhost");

for (const [name, csp] of [["production", production], ["development", development]]) {
  for (const [directive, sources] of csp.entries()) {
    assert(!sources.includes("*"), `${name} ${directive} must not allow *`);
    assert(!sources.includes("'unsafe-eval'"), `${name} ${directive} must not allow unsafe-eval`);
    if (name === "production") {
      assert(
        !sources.some((source) => /^https?:$/.test(source) || /^wss?:/.test(source) || source.includes("127.0.0.1:1420")),
        `production ${directive} must not allow remote/dev network sources`,
      );
    }
  }
}

process.stdout.write(JSON.stringify({ status: "passed", checked: ["csp", "devCsp"] }));

function normalizeCsp(value, label) {
  assert(value !== null && value !== undefined, `${label} must not be null`);
  if (typeof value === "string") {
    return new Map(value.split(";")
      .map((entry) => entry.trim())
      .filter(Boolean)
      .map((entry) => {
        const [directive, ...sources] = entry.split(/\s+/);
        return [directive, sources];
      }));
  }
  assert(typeof value === "object" && !Array.isArray(value), `${label} must be a string or object`);
  return new Map(Object.entries(value).map(([directive, sources]) => {
    if (Array.isArray(sources)) return [directive, sources.flatMap((source) => String(source).split(/\s+/)).filter(Boolean)];
    return [directive, String(sources).split(/\s+/).filter(Boolean)];
  }));
}

function assertSource(csp, directive, source) {
  assert(csp.get(directive)?.includes(source), `${directive} missing ${source}`);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
