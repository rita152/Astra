#!/usr/bin/env node

import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

const projectCwd = path.resolve(process.argv[2] ?? process.cwd());
const outputDir = path.resolve(
  process.argv[3] ?? path.join(projectCwd, "artifacts", "codex-app-server-capture"),
);
const prompt = process.argv.slice(4).join(" ") || "深入分析当前项目";
const startedAt = new Date();
const transcript = [];
const stderr = [];
let sequence = 0;
let threadId = null;
let turnId = null;
let completedTurn = null;
let fatalError = null;

await mkdir(outputDir, { recursive: true });

const child = spawn("codex", ["app-server", "--stdio"], {
  cwd: projectCwd,
  stdio: ["pipe", "pipe", "pipe"],
  env: process.env,
});

function classify(message) {
  if (message?.method && message?.id !== undefined) return "request";
  if (message?.method) return "notification";
  if (message?.id !== undefined && message?.error !== undefined) return "error_response";
  if (message?.id !== undefined) return "response";
  return "unknown";
}

function record(direction, message, rawLine = null, note = null) {
  transcript.push({
    sequence: ++sequence,
    timestamp: new Date().toISOString(),
    direction,
    kind: classify(message),
    method: typeof message?.method === "string" ? message.method : null,
    id: message?.id ?? null,
    note,
    message,
    rawLine,
  });
}

function send(message, note = null) {
  const rawLine = JSON.stringify(message);
  record("client_to_server", message, rawLine, note);
  child.stdin.write(`${rawLine}\n`);
}

function waitForResponse(id) {
  return new Promise((resolve, reject) => {
    const poll = () => {
      const entry = transcript.find(
        (candidate) =>
          candidate.direction === "server_to_client" &&
          candidate.id === id &&
          (candidate.kind === "response" || candidate.kind === "error_response"),
      );
      if (entry) {
        if (entry.message.error) {
          reject(new Error(`JSON-RPC request ${id} failed: ${JSON.stringify(entry.message.error)}`));
        } else {
          resolve(entry.message.result);
        }
        return;
      }
      if (fatalError) {
        reject(fatalError);
        return;
      }
      setTimeout(poll, 20);
    };
    poll();
  });
}

function waitForTurnCompletion() {
  return new Promise((resolve, reject) => {
    const poll = () => {
      if (completedTurn) {
        resolve(completedTurn);
        return;
      }
      if (fatalError) {
        reject(fatalError);
        return;
      }
      setTimeout(poll, 50);
    };
    poll();
  });
}

const stdoutReader = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
stdoutReader.on("line", (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch (error) {
    fatalError = new Error(`Invalid JSON from app-server: ${error.message}; line=${line}`);
    return;
  }
  record("server_to_client", message, line);

  if (message.method === "turn/started") {
    turnId = message.params?.turn?.id ?? turnId;
  } else if (message.method === "turn/completed") {
    completedTurn = message.params?.turn ?? { status: "unknown" };
  }

  if (message.method && message.id !== undefined) {
    send(
      {
        id: message.id,
        error: {
          code: -32601,
          message: `Capture client does not implement server request: ${message.method}`,
        },
      },
      `Automatic response to server-initiated request ${message.method}`,
    );
  }
});

const stderrReader = readline.createInterface({ input: child.stderr, crlfDelay: Infinity });
stderrReader.on("line", (line) => {
  stderr.push({ timestamp: new Date().toISOString(), line });
});

child.on("error", (error) => {
  fatalError = error;
});
child.on("exit", (code, signal) => {
  if (!completedTurn && !fatalError) {
    fatalError = new Error(`app-server exited before turn completion: code=${code} signal=${signal}`);
  }
});

let exitCode = null;
let exitSignal = null;
let runError = null;

try {
  send({
    method: "initialize",
    id: 1,
    params: {
      clientInfo: {
        name: "gpui_jsonrpc_capture",
        title: "GPUI JSON-RPC Capture",
        version: "1.0.0",
      },
    },
  });
  await waitForResponse(1);

  send({ method: "initialized", params: {} });
  send({
    method: "thread/start",
    id: 2,
    params: {
      cwd: projectCwd,
      approvalPolicy: "never",
      sandbox: "read-only",
      ephemeral: true,
      serviceName: "gpui-jsonrpc-capture",
    },
  });
  const threadResult = await waitForResponse(2);
  threadId = threadResult?.thread?.id ?? null;
  if (!threadId) throw new Error("thread/start response did not include result.thread.id");

  send({
    method: "turn/start",
    id: 3,
    params: {
      threadId,
      input: [{ type: "text", text: prompt }],
    },
  });
  const turnResult = await waitForResponse(3);
  turnId = turnResult?.turn?.id ?? turnId;
  await waitForTurnCompletion();
} catch (error) {
  runError = error instanceof Error ? error.message : String(error);
} finally {
  child.stdin.end();
  const exit = await new Promise((resolve) => {
    if (child.exitCode !== null || child.signalCode !== null) {
      resolve({ code: child.exitCode, signal: child.signalCode });
      return;
    }
    child.once("exit", (code, signal) => resolve({ code, signal }));
    setTimeout(() => child.kill("SIGTERM"), 2000);
  });
  exitCode = exit.code;
  exitSignal = exit.signal;
}

const finishedAt = new Date();
const methodMap = new Map();
for (const entry of transcript) {
  if (!entry.method) continue;
  const key = `${entry.direction}\u0000${entry.kind}\u0000${entry.method}`;
  const existing = methodMap.get(key) ?? {
    direction: entry.direction,
    kind: entry.kind,
    method: entry.method,
    count: 0,
    firstSequence: entry.sequence,
    ids: [],
  };
  existing.count += 1;
  if (entry.id !== null && !existing.ids.includes(entry.id)) existing.ids.push(entry.id);
  methodMap.set(key, existing);
}

const methods = [...methodMap.values()].sort((a, b) => a.firstSequence - b.firstSequence);
const serverMethods = methods.filter((entry) => entry.direction === "server_to_client");
const capture = {
  captureFormatVersion: 1,
  metadata: {
    command: ["codex", "app-server", "--stdio"],
    codexVersion: process.env.CODEX_CAPTURE_VERSION ?? null,
    projectCwd,
    prompt,
    startedAt: startedAt.toISOString(),
    finishedAt: finishedAt.toISOString(),
    durationMs: finishedAt.getTime() - startedAt.getTime(),
    threadId,
    turnId,
    turnStatus: completedTurn?.status ?? null,
    exitCode,
    exitSignal,
    runError,
  },
  transport: {
    framing: "newline-delimited JSON over stdio",
    note: "The observed app-server messages omit the optional jsonrpc field.",
    stderr,
  },
  transcript,
};

const summary = {
  captureFormatVersion: 1,
  source: "capture.json",
  metadata: capture.metadata,
  totals: {
    transcriptMessages: transcript.length,
    clientToServerMessages: transcript.filter((entry) => entry.direction === "client_to_server").length,
    serverToClientMessages: transcript.filter((entry) => entry.direction === "server_to_client").length,
    distinctMethodDirectionKindTriples: methods.length,
    distinctServerMethods: new Set(serverMethods.map((entry) => entry.method)).size,
  },
  methods,
  serverMethods,
};

await writeFile(path.join(outputDir, "capture.json"), `${JSON.stringify(capture, null, 2)}\n`);
await writeFile(path.join(outputDir, "methods.json"), `${JSON.stringify(summary, null, 2)}\n`);

if (runError) {
  console.error(runError);
  process.exitCode = 1;
} else {
  console.log(JSON.stringify({ outputDir, ...summary.totals, turnStatus: capture.metadata.turnStatus }));
}
