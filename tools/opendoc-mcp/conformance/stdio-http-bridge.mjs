#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
//
// A DEVELOPMENT bridge, not a product feature: it puts `opendoc-mcp` (stdio
// only, `docs/178` §6.1) behind a minimal HTTP endpoint so the official MCP
// conformance suite, which drives servers over HTTP, can be pointed at it
// (`docs/181` §7 R3):
//
//   cargo build -p opendoc-mcp
//   node tools/opendoc-mcp/conformance/stdio-http-bridge.mjs --port 3999 -- \
//     target/debug/opendoc-mcp --root /some/dir --profile review &
//   npx @modelcontextprotocol/conformance server --url http://127.0.0.1:3999/mcp \
//     --spec-version 2026-07-28
//
// Each POST body is one JSON-RPC message, written to the server as one line;
// a request's reply is the next line the server prints, returned as
// `application/json`. A notification gets 202. There is no SSE stream (GET is
// 405) and no session: opendoc-mcp is stateless in the 2026-07-28 sense. The
// bridge binds 127.0.0.1 only. What it adds is only the HTTP layer's part of
// Streamable HTTP: loopback Host/Origin checks, the SEP-2243 header-vs-body
// check, and the HTTP status for a JSON-RPC error. Every JSON-RPC answer is the
// server's, so a failure outside the HTTP-transport scenarios
// (`dns-rebinding-protection`, `http-*`, the `*-400`/`*-404` checks) is the
// server's.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { createInterface } from "node:readline";

const argv = process.argv.slice(2);
const split = argv.indexOf("--");
if (split < 1 || split === argv.length - 1) {
  process.stderr.write("usage: stdio-http-bridge.mjs --port N -- <opendoc-mcp> [args...]\n");
  process.exit(2);
}
const options = argv.slice(0, split);
const command = argv.slice(split + 1);
const port = Number(options[options.indexOf("--port") + 1] ?? 3999);

const child = spawn(command[0], command.slice(1), { stdio: ["pipe", "pipe", "inherit"] });
child.on("exit", (code) => {
  process.stderr.write(`opendoc-mcp exited (${code})\n`);
  process.exit(code ?? 1);
});

// Replies arrive in request order (the server answers one line at a time),
// so pending requests are a FIFO.
const waiting = [];
createInterface({ input: child.stdout }).on("line", (line) => {
  const next = waiting.shift();
  if (next) next(line);
});

function forward(message) {
  return new Promise((resolve) => {
    waiting.push(resolve);
    child.stdin.write(`${message}\n`);
  });
}

/** A loopback host or origin, the only ones a localhost bridge answers
 *  (DNS-rebinding protection, MCP Streamable HTTP security guidance). */
function loopback(value) {
  if (value === undefined) return true;
  try {
    const host = value.includes("://") ? new URL(value).hostname : new URL(`http://${value}`).hostname;
    return host === "127.0.0.1" || host === "localhost" || host === "[::1]";
  } catch {
    return false;
  }
}

/** The Streamable HTTP status for a JSON-RPC error the server returned
 *  (2026-07-28): method not found is 404; a malformed request, a header
 *  mismatch, a missing client capability and an unsupported version are 400. */
const HTTP_STATUS = new Map([
  [-32601, 404],
  [-32602, 400],
  [-32020, 400],
  [-32021, 400],
  [-32022, 400],
]);

/** SEP-2243: the `Mcp-Method`, `Mcp-Name` and `MCP-Protocol-Version` headers,
 *  when sent, must agree with the body. Returns the disagreement, or null. */
function headerMismatch(headers, message) {
  const header = (name) => (headers[name] === undefined ? undefined : String(headers[name]).trim());
  // A 2026-07-28 request must send them; an older client never does.
  const stateless = message.params?._meta?.["io.modelcontextprotocol/protocolVersion"] === "2026-07-28";
  const method = header("mcp-method");
  if (method === undefined ? stateless : method !== message.method) {
    return `Mcp-Method ${method ?? "(missing)"} does not match ${message.method}`;
  }
  const name = header("mcp-name");
  const target =
    message.method === "tools/call" || message.method === "prompts/get"
      ? message.params?.name
      : message.method === "resources/read"
        ? message.params?.uri
        : undefined;
  if (name === undefined ? stateless && target !== undefined : name !== target) {
    return `Mcp-Name ${name ?? "(missing)"} does not match the request`;
  }
  const version = header("mcp-protocol-version");
  const declared = message.params?._meta?.["io.modelcontextprotocol/protocolVersion"];
  if (version !== undefined && declared !== undefined && version !== declared) {
    return `MCP-Protocol-Version ${version} does not match _meta ${declared}`;
  }
  return null;
}

const server = createServer((request, response) => {
  if (!loopback(request.headers.host) || !loopback(request.headers.origin)) {
    response.writeHead(403).end();
    return;
  }
  if (request.method !== "POST") {
    response.writeHead(405, { allow: "POST" }).end();
    return;
  }
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", async () => {
    const body = Buffer.concat(chunks).toString("utf8");
    let message;
    try {
      message = JSON.parse(body);
    } catch {
      response
        .writeHead(400, { "content-type": "application/json" })
        .end(JSON.stringify({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "parse error" } }));
      return;
    }
    const isRequest = message && typeof message === "object" && "method" in message && "id" in message;
    if (!isRequest) {
      // A notification or a client response: the server prints nothing.
      child.stdin.write(`${JSON.stringify(message)}\n`);
      response.writeHead(202).end();
      return;
    }
    const mismatch = headerMismatch(request.headers, message);
    if (mismatch) {
      response
        .writeHead(400, { "content-type": "application/json" })
        .end(JSON.stringify({ jsonrpc: "2.0", id: message.id, error: { code: -32020, message: mismatch } }));
      return;
    }
    const reply = await forward(JSON.stringify(message));
    let status = 200;
    try {
      status = HTTP_STATUS.get(JSON.parse(reply)?.error?.code) ?? 200;
    } catch {
      // Not JSON: pass it through as the server wrote it.
    }
    response.writeHead(status, { "content-type": "application/json" }).end(reply);
  });
});
server.listen(port, "127.0.0.1", () => {
  process.stderr.write(`opendoc-mcp bridge on http://127.0.0.1:${port}/mcp\n`);
});
