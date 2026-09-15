import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import { join } from "node:path";
import { jwtVerify, createRemoteJWKSet, type JWTVerifyGetKey } from "jose";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";

const PASAPORTE_BASE_URL = "https://pasaportedelcafedeespecialidad.com";
const WRITE_TOOLS = new Set(["check_in", "submit_review", "log_visit"]);
const MAX_BODY = 256 * 1024;
const MAX_OUTPUT = 1024 * 1024;
const CHILD_TIMEOUT_MS = 15_000;
const MAX_CHILDREN = 4;
let children = 0;

export interface AdapterOptions {
  env?: NodeJS.ProcessEnv;
  binaryPath?: string;
  getKey?: JWTVerifyGetKey;
  childTimeoutMs?: number;
  maxOutputBytes?: number;
}

type Config = {
  resource: string;
  issuer: string;
  owner: string;
  login: string;
  password: string;
  allowWrites: boolean;
  metadataReady: boolean;
  dispatchReady: boolean;
};

function exactHttpsUrl(value: string | undefined, requiredPath?: string): URL | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) return undefined;
    if (requiredPath && url.pathname !== requiredPath) return undefined;
    return url;
  } catch { return undefined; }
}

function config(env: NodeJS.ProcessEnv): Config {
  const resourceUrl = exactHttpsUrl(env.PUBLIC_MCP_URL, "/mcp");
  const issuerUrl = exactHttpsUrl(env.AUTH0_ISSUER);
  const owner = env.MCP_OWNER_SUB ?? "";
  const login = env.PASAPORTE_LOGIN ?? "";
  const password = env.PASAPORTE_PASSWORD ?? "";
  const disabled = ["1", "true", "yes"].includes(env.PASAPORTE_DISABLE_CHECKIN ?? "");
  const allowed = ["1", "true", "yes"].includes(env.PASAPORTE_ALLOW_CHECKIN ?? "");
  return {
    resource: resourceUrl ? env.PUBLIC_MCP_URL! : "",
    issuer: issuerUrl ? env.AUTH0_ISSUER! : "",
    owner, login, password,
    allowWrites: allowed && !disabled,
    metadataReady: Boolean(resourceUrl && issuerUrl),
    dispatchReady: Boolean(resourceUrl && issuerUrl && owner.trim() && login.trim() && password),
  };
}

function metadataUrl(cfg: Config): string {
  const url = new URL(cfg.resource);
  url.pathname = "/.well-known/oauth-protected-resource";
  return url.href;
}

function challenge(cfg: Config): string {
  return `Bearer resource_metadata="${metadataUrl(cfg)}", scope="mcp:read"`;
}

function acquire(): void {
  if (children >= MAX_CHILDREN) throw new Error("bridge busy");
  children++;
}
function release(): void { children--; }

async function defaultBinary(): Promise<string> {
  const packaged = join(process.cwd(), "hosted", "bin", "pasaporte-cafe-mcp");
  await access(packaged);
  return packaged;
}

async function rustRpc(
  method: "tools/list" | "tools/call",
  params: unknown,
  cfg: Config,
  signal: AbortSignal,
  options: AdapterOptions,
): Promise<Record<string, unknown>> {
  acquire();
  try {
    const binary = options.binaryPath ?? await defaultBinary();
    if (signal.aborted) throw new Error("bridge aborted");
    const rpc = JSON.stringify({ jsonrpc: "2.0", id: 1, method, ...(params === undefined ? {} : { params }) });
    if (Buffer.byteLength(rpc) > MAX_BODY) throw new Error("bridge request too large");
    return await new Promise((resolve, reject) => {
      const child = spawn(binary, [], {
        env: {
          PASAPORTE_BASE_URL,
          PASAPORTE_LOGIN: cfg.login,
          PASAPORTE_PASSWORD: cfg.password,
          PASAPORTE_ALLOW_CHECKIN: cfg.allowWrites ? "1" : "0",
          PASAPORTE_DISABLE_CHECKIN: cfg.allowWrites ? "0" : "1",
        },
        stdio: ["pipe", "pipe", "ignore"],
      });
      const chunks: Buffer[] = [];
      let size = 0;
      let settled = false;
      const fail = () => { if (!settled) { settled = true; child.kill("SIGKILL"); reject(new Error("bridge failed")); } };
      const timer = setTimeout(fail, options.childTimeoutMs ?? CHILD_TIMEOUT_MS);
      const abort = () => fail();
      signal.addEventListener("abort", abort, { once: true });
      child.stdout.on("data", (chunk: Buffer) => {
        size += chunk.length;
        if (size > (options.maxOutputBytes ?? MAX_OUTPUT)) fail(); else chunks.push(chunk);
      });
      child.stdin.on("error", fail);
      child.on("error", fail);
      child.on("close", (code) => {
        clearTimeout(timer); signal.removeEventListener("abort", abort);
        if (settled) return;
        settled = true;
        if (code !== 0) return reject(new Error("bridge failed"));
        try {
          const lines = Buffer.concat(chunks).toString("utf8").trim().split("\n");
          if (lines.length !== 1) throw new Error();
          const response = JSON.parse(lines[0]) as { result?: Record<string, unknown>; error?: unknown };
          if (!response.result || response.error) throw new Error();
          resolve(response.result);
        } catch { reject(new Error("bridge failed")); }
      });
      child.stdin.end(`${rpc}\n`);
    });
  } finally { release(); }
}

function decorateTools(result: Record<string, unknown>, cfg: Config, scopes: Set<string>): Record<string, unknown> {
  const tools = Array.isArray(result.tools) ? result.tools : [];
  return {
    ...result,
    tools: tools
      .filter((tool: any) => !WRITE_TOOLS.has(tool?.name) || (cfg.allowWrites && scopes.has("mcp:write")))
      .map((tool: any) => {
        const write = WRITE_TOOLS.has(tool.name);
        const scopes = write ? ["mcp:read", "mcp:write"] : ["mcp:read"];
        return {
          ...tool,
          annotations: {
            readOnlyHint: !write,
            destructiveHint: write,
            idempotentHint: !write,
            openWorldHint: true,
          },
          securitySchemes: [{ type: "oauth2", scopes }],
        };
      }),
  };
}

async function verify(request: Request, cfg: Config, getKey: JWTVerifyGetKey): Promise<Set<string>> {
  const match = /^Bearer ([^\s]+)$/.exec(request.headers.get("authorization") ?? "");
  if (!match) throw new Error("unauthorized");
  const issuer = cfg.issuer;
  const { payload, protectedHeader } = await jwtVerify(match[1], getKey, {
    algorithms: ["RS256"], issuer, audience: cfg.resource,
    requiredClaims: ["exp", "sub"],
  });
  if (protectedHeader.alg !== "RS256" || payload.sub !== cfg.owner) throw new Error("unauthorized");
  const scopes = new Set(typeof payload.scope === "string" ? payload.scope.split(/\s+/).filter(Boolean) : []);
  if (!scopes.has("mcp:read")) throw new Error("unauthorized");
  return scopes;
}

async function limitedJson(request: Request): Promise<unknown> {
  const reader = request.body?.getReader();
  if (!reader) throw new Error("missing body");
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY) { await reader.cancel(); throw new Error("too large"); }
    chunks.push(value);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

export function createHandler(options: AdapterOptions = {}) {
  const env = options.env ?? process.env;
  let cachedIssuer = "";
  let cachedKey: JWTVerifyGetKey | undefined;
  return async (request: Request): Promise<Response> => {
    const cfg = config(env);
    const pathname = new URL(request.url).pathname;
    if (pathname === "/.well-known/oauth-protected-resource") {
      if (request.method !== "GET") return new Response("Method Not Allowed", { status: 405 });
      if (!cfg.metadataReady) return Response.json({ error: "service unavailable" }, { status: 503 });
      return Response.json({ resource: cfg.resource, authorization_servers: [cfg.issuer], scopes_supported: ["mcp:read", "mcp:write"] });
    }
    if (pathname !== "/mcp") return new Response("Not Found", { status: 404 });
    if (!cfg.dispatchReady) return Response.json({ error: "service unavailable" }, { status: 503 });
    const origin = request.headers.get("origin");
    if (origin && origin !== new URL(cfg.resource).origin) return new Response("Forbidden", { status: 403 });
    let scopes: Set<string>;
    try {
      if (!options.getKey && (!cachedKey || cachedIssuer !== cfg.issuer)) {
        cachedKey = createRemoteJWKSet(new URL("/.well-known/jwks.json", cfg.issuer));
        cachedIssuer = cfg.issuer;
      }
      scopes = await verify(request, cfg, options.getKey ?? cachedKey!);
    }
    catch { return Response.json({ error: "unauthorized" }, { status: 401, headers: { "WWW-Authenticate": challenge(cfg) } }); }
    if (request.method !== "POST") return new Response("Method Not Allowed", { status: 405 });
    let body: any;
    try { body = await limitedJson(request); }
    catch { return Response.json({ error: "invalid request" }, { status: 400 }); }
    if (body?.method === "tools/call") {
      const name = body?.params?.name;
      if (WRITE_TOOLS.has(name) && (!cfg.allowWrites || !scopes.has("mcp:write"))) {
        return Response.json({ error: "forbidden" }, { status: 403 });
      }
    }
    const server = new Server({ name: "pasaporte-cafe-hosted", version: "0.1.0" }, { capabilities: { tools: {} } });
    server.setRequestHandler(ListToolsRequestSchema, async (_request, extra) => {
      try { return decorateTools(await rustRpc("tools/list", undefined, cfg, AbortSignal.any([extra.signal, request.signal]), options), cfg, scopes) as any; }
      catch { throw new Error("tool bridge unavailable"); }
    });
    server.setRequestHandler(CallToolRequestSchema, async ({ params }, extra) => {
      const write = WRITE_TOOLS.has(params.name);
      if (write && (!cfg.allowWrites || !scopes.has("mcp:write"))) throw new Error("forbidden");
      try { return await rustRpc("tools/call", params, cfg, AbortSignal.any([extra.signal, request.signal]), options) as any; }
      catch { throw new Error("tool bridge unavailable"); }
    });
    const transport = new WebStandardStreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true });
    await server.connect(transport);
    try { return await transport.handleRequest(request, { parsedBody: body }); }
    finally { await server.close(); }
  };
}
