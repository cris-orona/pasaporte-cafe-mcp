import test from "node:test";
import assert from "node:assert/strict";
import { access, chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { generateKeyPair, SignJWT } from "jose";
import { createHandler, type AdapterOptions } from "../adapter.js";

const resource = "https://coffee.example/mcp";
const issuer = "https://tenant.example/";
const owner = "auth0|owner";
const baseEnv = {
  PUBLIC_MCP_URL: resource,
  AUTH0_ISSUER: issuer,
  MCP_OWNER_SUB: owner,
  PASAPORTE_LOGIN: "synthetic-login",
  PASAPORTE_PASSWORD: "synthetic-password",
};
const { privateKey, publicKey } = await generateKeyPair("RS256");

async function token(overrides: Record<string, unknown> = {}, alg = "RS256"): Promise<string> {
  const now = Math.floor(Date.now() / 1000);
  const claims = { iss: issuer, aud: resource, sub: owner, scope: "mcp:read", exp: now + 300, ...overrides };
  if (alg === "RS256") return new SignJWT(claims).setProtectedHeader({ alg }).sign(privateKey);
  return new SignJWT(claims).setProtectedHeader({ alg: "HS256" }).sign(new TextEncoder().encode("not-an-rsa-key-not-an-rsa-key"));
}

function request(body: unknown, bearer?: string, extra: HeadersInit = {}): Request {
  const headers = new Headers({ "content-type": "application/json", accept: "application/json, text/event-stream", ...extra });
  if (bearer) headers.set("authorization", `Bearer ${bearer}`);
  return new Request(resource, { method: "POST", headers, body: JSON.stringify(body) });
}

function handler(env: NodeJS.ProcessEnv = baseEnv, options: AdapterOptions = {}) {
  return createHandler({ env, getKey: async () => publicKey, ...options });
}

const listRpc = { jsonrpc: "2.0", id: 1, method: "tools/list", params: {} };

await test("configuration is deny-all and metadata is canonical", async () => {
  const unavailable = await handler({})(request(listRpc, await token()));
  assert.equal(unavailable.status, 503);
  const metadataHandler = handler({ PUBLIC_MCP_URL: resource, AUTH0_ISSUER: issuer });
  const metadata = await metadataHandler(new Request("https://attacker.invalid/.well-known/oauth-protected-resource", { headers: { host: "attacker.invalid", origin: "https://evil.invalid" } }));
  assert.equal(metadata.status, 200);
  assert.deepEqual(await metadata.json(), { resource, authorization_servers: [issuer], scopes_supported: ["mcp:read", "mcp:write"] });
});

await test("rejects missing and invalid access tokens before spawn", async () => {
  const { privateKey: attackerKey } = await generateKeyPair("RS256");
  const forged = await new SignJWT({ iss: issuer, aud: resource, sub: owner, scope: "mcp:read", exp: Math.floor(Date.now() / 1000) + 300 })
    .setProtectedHeader({ alg: "RS256" }).sign(attackerKey);
  const cases: Array<[string, string | undefined]> = [
    ["forged signature", forged],
    ["missing", undefined], ["malformed", "nope"], ["expired", await token({ exp: 1 })],
    ["wrong algorithm", await token({}, "HS256")], ["wrong issuer", await token({ iss: "https://wrong.example/" })],
    ["wrong audience / ID token", await token({ aud: "client-id" })], ["wrong owner", await token({ sub: "auth0|other" })],
    ["missing scope", await token({ scope: "openid profile" })], ["missing exp", await token({ exp: undefined })],
    ["future nbf", await token({ nbf: Math.floor(Date.now() / 1000) + 300 })],
  ];
  for (const [name, bearer] of cases) {
    const response = await handler(baseEnv, { binaryPath: "/must-not-spawn" })(request(listRpc, bearer));
    assert.equal(response.status, 401, name);
    assert.match(response.headers.get("www-authenticate") ?? "", /https:\/\/coffee\.example\/\.well-known\/oauth-protected-resource/);
  }
});

await test("each required setting fails closed when missing", async () => {
  for (const key of Object.keys(baseEnv)) {
    const env: NodeJS.ProcessEnv = { ...baseEnv };
    delete env[key];
    assert.equal((await handler(env, { binaryPath: "/must-not-spawn" })(request(listRpc, await token()))).status, 503, key);
  }
});

await test("actual Rust bridge initializes and lists authoritative annotated tools", async () => {
  const h = handler(baseEnv, { binaryPath: "target/debug/pasaporte-cafe-mcp" });
  const init = await h(request({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "test", version: "1" } } }, await token()));
  assert.equal(init.status, 200);
  assert.equal((await init.json() as any).result.serverInfo.name, "pasaporte-cafe-hosted");
  const listed = await h(request(listRpc, await token()));
  assert.equal(listed.status, 200);
  const tools = (await listed.json() as any).result.tools;
  assert.equal(tools.length, 9);
  assert.ok(tools.some((tool: any) => tool.name === "cafe_directory"));
  assert.ok(tools.every((tool: any) => tool.annotations.readOnlyHint === true));
  assert.deepEqual(tools[0].securitySchemes, [{ type: "oauth2", scopes: ["mcp:read"] }]);
});

await test("write listing and direct calls require both scope and operator opt-in", async () => {
  const enabled = { ...baseEnv, PASAPORTE_ALLOW_CHECKIN: "1" };
  const readList = await handler(enabled, { binaryPath: "target/debug/pasaporte-cafe-mcp" })(request(listRpc, await token()));
  assert.equal((await readList.json() as any).result.tools.length, 9);
  const writeList = await handler(enabled, { binaryPath: "target/debug/pasaporte-cafe-mcp" })(request(listRpc, await token({ scope: "mcp:read mcp:write" })));
  const writeTools = (await writeList.json() as any).result.tools;
  assert.ok(writeTools.some((tool: any) => tool.name === "check_in" && tool.annotations.destructiveHint === true));
  const denied = await handler(enabled, { binaryPath: "/must-not-spawn" })(request({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "check_in", arguments: {} } }, await token()));
  assert.equal(denied.status, 403);
  const disabled = { ...enabled, PASAPORTE_DISABLE_CHECKIN: "1" };
  const hidden = await handler(disabled, { binaryPath: "target/debug/pasaporte-cafe-mcp" })(request(listRpc, await token({ scope: "mcp:read mcp:write" })));
  assert.equal((await hidden.json() as any).result.tools.length, 9);
  const disabledCall = await handler(disabled, { binaryPath: "/must-not-spawn" })(request({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "check_in", arguments: {} } }, await token({ scope: "mcp:read mcp:write" })));
  assert.equal(disabledCall.status, 403);
});

async function script(contents: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "pasaporte-bridge-"));
  const path = join(dir, "fake.sh");
  await writeFile(path, `#!/bin/sh\n${contents}\n`);
  await chmod(path, 0o700);
  return path;
}

await test("malicious Origin is rejected and bridge failures are generic", async () => {
  const origin = await handler(baseEnv, { binaryPath: "/must-not-spawn" })(request(listRpc, await token(), { origin: "https://evil.invalid", host: "evil.invalid" }));
  assert.equal(origin.status, 403);
  for (const binaryPath of [await script("echo supersecret >&2; exit 9"), await script("sleep 2"), await script("printf '{\\\"result\\\":{\\\"tools\\\":[]}}\\n'; head -c 2000 /dev/zero")]) {
    const response = await handler(baseEnv, { binaryPath, childTimeoutMs: 30, maxOutputBytes: 256 })(request(listRpc, await token()));
    assert.equal(response.status, 200);
    const body = await response.json() as any;
    assert.match(body.error.message, /tool bridge unavailable/);
    assert.doesNotMatch(JSON.stringify(body), /exit|SIG|sleep|zero|Rust|supersecret/i);
  }
});

await test("aborted requests do not start a Rust process", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "pasaporte-abort-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const marker = join(dir, "started");
  const binaryPath = join(dir, "fake.sh");
  await writeFile(binaryPath, `#!/bin/sh\necho started > '${marker}'\nprintf '{"result":{"tools":[]}}\\n'\n`);
  await chmod(binaryPath, 0o700);
  const controller = new AbortController();
  const req = new Request(request(listRpc, await token()), { signal: controller.signal });
  controller.abort();
  const response = await handler(baseEnv, { binaryPath })(req);
  assert.match(JSON.stringify(await response.json()), /tool bridge unavailable/);
  await assert.rejects(access(marker), { code: "ENOENT" });
});

await test("oversized requests and all unapproved write names are rejected", async () => {
  const h = handler(baseEnv, { binaryPath: "/must-not-spawn" });
  assert.equal((await h(request({ ...listRpc, padding: "x".repeat(300_000) }, await token()))).status, 400);
  for (const name of ["check_in", "submit_review", "log_visit"]) {
    assert.equal((await h(request({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name, arguments: {} } }, await token({ scope: "mcp:read mcp:write" })))).status, 403);
  }
});
