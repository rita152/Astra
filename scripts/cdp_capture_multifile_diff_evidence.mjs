#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

const CDP_HTTP = "http://127.0.0.1:9222";
const CHATGPT_APP = "/Applications/ChatGPT.app";
const APP_ASAR = path.join(CHATGPT_APP, "Contents", "Resources", "app.asar");
const CODEX_BIN = path.join(CHATGPT_APP, "Contents", "Resources", "codex");
const INFO_PLIST = path.join(CHATGPT_APP, "Contents", "Info.plist");
const DEFAULT_ARTIFACT_DIR = path.resolve(
  "artifacts/chatgpt-multifile-diff-cdp-audit-2026-08-30",
);
const OBSERVER_GLOBAL = "__codexMultifileDiffEvidenceV1";
const NATURAL_SCENARIO_PATHS = [
  "/tmp/codex-cdp-multifile-natural-20260830-a.txt",
  "/tmp/codex-cdp-multifile-natural-20260830-b.txt",
];
const APPROVAL_RETRY_PATHS = [
  "/tmp/codex-cdp-multifile-approval-20260830-a.txt",
  "/tmp/codex-cdp-multifile-approval-20260830-b.txt",
];
const EXTERNAL_APPROVAL_PATHS = [
  "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-a.txt",
  "/Users/zp/Desktop/codex-cdp-multifile-approval-20260830-b.txt",
];
const MANY_EXTERNAL_APPROVAL_PATHS = Array.from(
  { length: 8 },
  (_, index) =>
    `/Users/zp/Desktop/codex-cdp-multifile-scroll-20260830-${String(index + 1).padStart(2, "0")}.txt`,
);

const rawArgs = process.argv.slice(2);
const command = rawArgs[0] ?? "help";
const positional = rawArgs.slice(1).filter((argument) => !argument.startsWith("--"));
const option = (name) => {
  const prefix = `--${name}=`;
  return rawArgs.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? null;
};
const artifactDir = path.resolve(option("artifact-dir") ?? DEFAULT_ARTIFACT_DIR);

const usage = `Usage:
  node scripts/cdp_capture_multifile_diff_evidence.mjs offline [--artifact-dir=DIR]
  node scripts/cdp_capture_multifile_diff_evidence.mjs scenario-plan [--artifact-dir=DIR]
  node scripts/cdp_capture_multifile_diff_evidence.mjs self-test
  node scripts/cdp_capture_multifile_diff_evidence.mjs submit-scenario 1|2
  node scripts/cdp_capture_multifile_diff_evidence.mjs submit-approval-retry
  node scripts/cdp_capture_multifile_diff_evidence.mjs submit-external-approval
  node scripts/cdp_capture_multifile_diff_evidence.mjs submit-many-external-approval
  node scripts/cdp_capture_multifile_diff_evidence.mjs install
  node scripts/cdp_capture_multifile_diff_evidence.mjs observer-status
  node scripts/cdp_capture_multifile_diff_evidence.mjs clear-events
  node scripts/cdp_capture_multifile_diff_evidence.mjs events [NAME]
  node scripts/cdp_capture_multifile_diff_evidence.mjs capture NAME
  node scripts/cdp_capture_multifile_diff_evidence.mjs click SELECTOR [INDEX]
  node scripts/cdp_capture_multifile_diff_evidence.mjs click-text TEXT [SELECTOR]
  node scripts/cdp_capture_multifile_diff_evidence.mjs hover SELECTOR [INDEX]
  node scripts/cdp_capture_multifile_diff_evidence.mjs hover-text TEXT [SELECTOR]
  node scripts/cdp_capture_multifile_diff_evidence.mjs focus SELECTOR [INDEX]
  node scripts/cdp_capture_multifile_diff_evidence.mjs press KEY [shift]
  node scripts/cdp_capture_multifile_diff_evidence.mjs press-capture KEY NAME [shift]
  node scripts/cdp_capture_multifile_diff_evidence.mjs insert TEXT
  node scripts/cdp_capture_multifile_diff_evidence.mjs scroll SELECTOR TOP [LEFT]
  node scripts/cdp_capture_multifile_diff_evidence.mjs evaluate EXPRESSION

The observer is passive: it listens to incoming window messages and the app's
codex-message-from-view event. It does not replace Electron bridge functions.
`;

if (command === "help" || command === "--help" || command === "-h") {
  process.stdout.write(usage);
  process.exit(0);
}

function ensureArtifactDir() {
  fs.mkdirSync(artifactDir, { recursive: true });
}

function readAsarIndex(asarPath) {
  const fd = fs.openSync(asarPath, "r");
  try {
    const prefix = Buffer.alloc(16);
    fs.readSync(fd, prefix, 0, prefix.length, 0);
    const headerLength = prefix.readUInt32LE(12);
    const header = Buffer.alloc(headerLength);
    fs.readSync(fd, header, 0, header.length, 16);
    return {
      contentOffset: 16 + headerLength,
      fd,
      header: JSON.parse(header.toString("utf8")),
      headerLength,
    };
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
}

function findAsarNode(header, filePath) {
  let node = header;
  for (const part of filePath.split("/")) {
    node = node.files?.[part];
    if (!node) throw new Error(`ASAR entry not found: ${filePath}`);
  }
  return node;
}

function readAsarFile(index, filePath) {
  const node = findAsarNode(index.header, filePath);
  if (node.unpacked) throw new Error(`ASAR entry is unpacked: ${filePath}`);
  const buffer = Buffer.alloc(node.size);
  fs.readSync(
    index.fd,
    buffer,
    0,
    buffer.length,
    index.contentOffset + Number(node.offset),
  );
  return { buffer, node };
}

function allAsarFiles(header) {
  const result = [];
  const visit = (node, parts) => {
    if (node.files) {
      for (const [name, child] of Object.entries(node.files)) {
        visit(child, [...parts, name]);
      }
      return;
    }
    result.push({ path: parts.join("/"), ...node });
  };
  visit(header, []);
  return result;
}

function allIndices(source, needle) {
  const indices = [];
  let index = -1;
  while ((index = source.indexOf(needle, index + 1)) >= 0) indices.push(index);
  return indices;
}

function evidenceSnippet(source, needle, before = 900, after = 2400) {
  const index = source.indexOf(needle);
  if (index < 0) return null;
  return {
    index,
    needle,
    snippet: source.slice(Math.max(0, index - before), index + needle.length + after),
  };
}

function plistValue(key) {
  return execFileSync("plutil", ["-extract", key, "raw", INFO_PLIST], {
    encoding: "utf8",
  }).trim();
}

function baselineLines(label) {
  return Array.from(
    { length: 48 },
    (_, index) => `${label}-${String(index + 1).padStart(2, "0")} baseline context line`,
  );
}

function updatedLines(label) {
  const replacementLines =
    label === "A" ? new Set([4, 11, 18, 25, 32, 39, 46]) : new Set([3, 10, 17, 24, 31, 38, 45]);
  const insertionAfter = label === "A" ? 20 : 21;
  const deletedLine = label === "A" ? 42 : 43;
  const result = [];
  for (let line = 1; line <= 48; line += 1) {
    if (line !== deletedLine) {
      const suffix = replacementLines.has(line) ? "revised changed line" : "baseline context line";
      result.push(`${label}-${String(line).padStart(2, "0")} ${suffix}`);
    }
    if (line === insertionAfter) {
      result.push(`${label}-${String(line).padStart(2, "0")}.5 inserted evidence line`);
    }
  }
  return result;
}

function addFilePatch(filePath, label) {
  return [
    `*** Add File: ${filePath}`,
    ...baselineLines(label).map((line) => `+${line}`),
  ].join("\n");
}

function updateFilePatch(filePath, label) {
  const replacements =
    label === "A" ? new Set([4, 11, 18, 25, 32, 39, 46]) : new Set([3, 10, 17, 24, 31, 38, 45]);
  const insertionAfter = label === "A" ? 20 : 21;
  const deletedLine = label === "A" ? 42 : 43;
  const lines = [`*** Update File: ${filePath}`, "@@"];
  for (let line = 1; line <= 48; line += 1) {
    const number = String(line).padStart(2, "0");
    const baseline = `${label}-${number} baseline context line`;
    if (replacements.has(line)) {
      lines.push(`-${baseline}`, `+${label}-${number} revised changed line`);
    } else if (line === deletedLine) {
      lines.push(`-${baseline}`);
    } else {
      lines.push(` ${baseline}`);
    }
    if (line === insertionAfter) {
      lines.push(`+${label}-${number}.5 inserted evidence line`);
    }
  }
  return lines.join("\n");
}

function naturalScenarioPlan() {
  const [fileA, fileB] = NATURAL_SCENARIO_PATHS;
  const createPatch = [
    "*** Begin Patch",
    addFilePatch(fileA, "A"),
    addFilePatch(fileB, "B"),
    "*** End Patch",
  ].join("\n");
  const updatePatch = [
    "*** Begin Patch",
    updateFilePatch(fileA, "A"),
    updateFilePatch(fileB, "B"),
    "*** End Patch",
  ].join("\n");
  return {
    generatedAt: new Date().toISOString(),
    evidenceRule:
      "Submit these prompts through the normal ChatGPT App composer. Do not dispatch synthetic MCP messages or mutate renderer state.",
    prerequisites: [
      "Both fixture paths must be absent before prompt 1.",
      "The composer permission mode must be Request approval.",
      "The passive protocol observer must be installed and cleared before prompt 1.",
      "Choose Allow once for prompt 1 so prompt 2 independently requests approval.",
    ],
    fixturePaths: [fileA, fileB],
    prompt1: [
      "这是一次真实文件审批 UI 取证。请只调用一次内置 apply_patch 文件修改工具，严格使用下面的补丁一次创建两个文件。",
      "不要使用 shell、终端、重定向、脚本或其他写文件方式；不要拆成两个工具调用；不要仅解释。请直接发起系统文件修改审批并等待我处理。",
      "",
      createPatch,
    ].join("\n"),
    prompt2: [
      "继续同一真实 UI 取证。请只调用一次内置 apply_patch 文件修改工具，严格使用下面的单个补丁同时更新两个文件。",
      "不要使用 shell、终端、重定向、脚本或其他写文件方式；不要拆分工具调用；不要重写整文件；不要仅解释。请直接发起系统文件修改审批并等待我处理。",
      "",
      updatePatch,
    ].join("\n"),
    expected: {
      firstApproval: {
        fileCount: 2,
        perFile: [
          { path: fileA, additions: 48, deletions: 0 },
          { path: fileB, additions: 48, deletions: 0 },
        ],
      },
      secondApproval: {
        fileCount: 2,
        totalAdditions: 16,
        totalDeletions: 16,
        perFile: [
          { path: fileA, additions: 8, deletions: 8 },
          { path: fileB, additions: 8, deletions: 8 },
        ],
      },
      finalContents: {
        [fileA]: `${updatedLines("A").join("\n")}\n`,
        [fileB]: `${updatedLines("B").join("\n")}\n`,
      },
      layoutIntent:
        "Seven regularly spaced replacements plus one pure insertion and deletion per file force a long unified diff with added, deleted, and context rows across two repeated file regions.",
    },
  };
}

function approvalRetryPrompt(paths = APPROVAL_RETRY_PATHS) {
  const [fileA, fileB] = paths;
  return [
    "请使用内置文件修改工具，在同一个文件修改项中创建下面两个文件，并在系统文件修改审批界面出现后停止、等待我操作。",
    "不要使用 shell、终端、重定向或脚本；不要自行批准；不要把两个文件拆成两个工具调用。",
    `${fileA}：写入一行 APPROVAL_A`,
    `${fileB}：写入一行 APPROVAL_B`,
  ].join("\n");
}

function manyExternalApprovalPrompt() {
  return [
    "请使用内置文件修改工具，在同一个文件修改项中创建下面八个文件，并在系统文件修改审批界面出现后停止、等待我操作。",
    "不要使用 shell、终端、重定向或脚本；不要自行批准；不要把文件拆成多个工具调用。",
    ...MANY_EXTERNAL_APPROVAL_PATHS.map(
      (filePath, index) => `${filePath}：写入一行 SCROLL_${String(index + 1).padStart(2, "0")}`,
    ),
  ].join("\n");
}

function writeNaturalScenarioPlan() {
  ensureArtifactDir();
  const plan = naturalScenarioPlan();
  const output = path.join(artifactDir, "01-natural-scenario-plan.json");
  fs.writeFileSync(output, `${JSON.stringify(plan, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify({ output, expected: plan.expected }, null, 2)}\n`);
}

function generateOfflineEvidence() {
  ensureArtifactDir();
  const asar = readAsarIndex(APP_ASAR);
  try {
    const indexHtml = readAsarFile(asar, "webview/index.html").buffer.toString("utf8");
    const rendererMatch = indexHtml.match(/src="\.\/assets\/(index-[^"]+\.js)"/);
    if (!rendererMatch) throw new Error("Renderer entry was not found in webview/index.html");

    const candidateFiles = allAsarFiles(asar.header).filter(
      (entry) =>
        !entry.unpacked &&
        /^(?:webview\/assets|\.vite\/build)\/.*\.(?:js|mjs)$/.test(entry.path),
    );
    const method = "item/fileChange/requestApproval";
    const matchingFiles = [];
    for (const entry of candidateFiles) {
      const { buffer, node } = readAsarFile(asar, entry.path);
      const source = buffer.toString("utf8");
      if (!source.includes(method)) continue;
      matchingFiles.push({
        path: entry.path,
        size: node.size,
        offset: node.offset,
        integrity: node.integrity ?? null,
        methodOffsets: allIndices(source, method),
      });
    }

    const appInitialEntry = matchingFiles
      .filter((entry) => entry.path.startsWith("webview/assets/app-initial-"))
      .sort((a, b) => b.size - a.size)[0];
    if (!appInitialEntry) throw new Error("App initial bundle containing file approval was not found");
    const appInitialSource = readAsarFile(asar, appInitialEntry.path).buffer.toString("utf8");
    const preloadPath = ".vite/build/preload.js";
    const preloadFile = readAsarFile(asar, preloadPath);
    const preloadSource = preloadFile.buffer.toString("utf8");

    const schemaDir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-app-schema-"));
    execFileSync(
      CODEX_BIN,
      ["app-server", "generate-json-schema", "--experimental", "--out", schemaDir],
      { stdio: "pipe" },
    );
    const schemaFiles = [
      "FileChangeRequestApprovalParams.json",
      "FileChangeRequestApprovalResponse.json",
      "v2/FileChangePatchUpdatedNotification.json",
      "v2/TurnDiffUpdatedNotification.json",
      "v2/ItemStartedNotification.json",
      "v2/ItemCompletedNotification.json",
    ];
    const schemas = Object.fromEntries(
      schemaFiles.map((relativePath) => [
        relativePath,
        JSON.parse(fs.readFileSync(path.join(schemaDir, relativePath), "utf8")),
      ]),
    );

    const rendererEvidence = [
      evidenceSnippet(appInitialSource, "onRequest(e){let{id:t,method:n,params:r}=e", 300, 5600),
      evidenceSnippet(
        appInitialSource,
        "case`item/fileChange/requestApproval`:case`item/commandExecution/requestApproval`",
        500,
        2600,
      ),
      evidenceSnippet(appInitialSource, "function Mct(e,t,n,r,i)", 200, 2100),
      evidenceSnippet(appInitialSource, "codex-message-from-view", 900, 1400),
      evidenceSnippet(appInitialSource, "case`mcp-request`:f(e.hostId)?.onRequest", 900, 1300),
      evidenceSnippet(appInitialSource, "function zAc(e){let t=new Map", 200, 1500),
    ].filter(Boolean);
    const renderingEvidence = [
      evidenceSnippet(appInitialSource, "function mWc(e)", 3400, 8600),
      evidenceSnippet(
        appInitialSource,
        "patchApprovalRequest.prompt.chatgpt.files",
        1800,
        4800,
      ),
      evidenceSnippet(appInitialSource, "data-codex-approval-surface", 1800, 900),
      evidenceSnippet(appInitialSource, "data-app-action-review-file-toggle", 1000, 2600),
      evidenceSnippet(appInitialSource, "data-app-action-review-scroll", 1000, 2600),
      evidenceSnippet(appInitialSource, "function mOs(e)", 500, 7200),
      evidenceSnippet(appInitialSource, "function HAo(e,t)", 1400, 3400),
      evidenceSnippet(appInitialSource, "...Wp.reviewScroll", 1700, 3300),
    ].filter(Boolean);

    const evidence = {
      capturedAt: new Date().toISOString(),
      application: {
        bundleIdentifier: plistValue("CFBundleIdentifier"),
        shortVersion: plistValue("CFBundleShortVersionString"),
        buildVersion: plistValue("CFBundleVersion"),
        codexVersion: execFileSync(CODEX_BIN, ["--version"], { encoding: "utf8" }).trim(),
        appAsar: APP_ASAR,
      },
      asar: {
        headerLength: asar.headerLength,
        contentOffset: asar.contentOffset,
        rendererEntry: `webview/assets/${rendererMatch[1]}`,
        method,
        matchingFiles,
        appInitial: {
          ...appInitialEntry,
          sha256: crypto.createHash("sha256").update(appInitialSource).digest("hex"),
        },
        preload: {
          path: preloadPath,
          size: preloadFile.node.size,
          offset: preloadFile.node.offset,
          integrity: preloadFile.node.integrity ?? null,
          sha256: crypto.createHash("sha256").update(preloadSource).digest("hex"),
        },
      },
      callsites: {
        rendererEvidence,
        renderingEvidence,
        preloadBridge: [
          evidenceSnippet(preloadSource, "sendMessageFromView:async", 300, 1700),
          evidenceSnippet(preloadSource, "subscribeToWorkerMessages:", 500, 1300),
        ].filter(Boolean),
      },
      protocol: {
        generatedWithExperimentalFlag: true,
        schemaFiles,
        schemas,
        inference: [
          "FileChangeRequestApprovalParams identifies the item but does not repeat its changes.",
          "Per-file paths and per-file diffs arrive on item/started or item/fileChange/patchUpdated.",
          "turn/diff/updated carries the latest aggregated unified diff across the turn.",
          "The renderer stores the request and resolves it with result.decision.",
          "The real patch approval title pluralizes from Object.keys(item.changes).length.",
          "The real patch approval body maps Object.entries(item.changes), so one approval item can render multiple file rows.",
          "The real patch approval file-list container has max-height 200px and vertical overflow scrolling.",
          "Review files expose canonical data-review-path, data-app-action-review-file-toggle, and data-app-action-review-scroll attributes.",
          "The review viewport is a full-height vertical scroller and each full-review file header can be sticky at top 0.",
          "The real review line lookup explicitly opts into Shadow Root traversal, so evidence collection must do the same.",
        ],
      },
    };
    const output = path.join(artifactDir, "00-offline-bundle-schema-evidence.json");
    fs.writeFileSync(output, `${JSON.stringify(evidence, null, 2)}\n`);
    fs.rmSync(schemaDir, { recursive: true, force: true });
    process.stdout.write(
      `${JSON.stringify(
        {
          output,
          appVersion: evidence.application.shortVersion,
          buildVersion: evidence.application.buildVersion,
          codexVersion: evidence.application.codexVersion,
          matchingFiles: matchingFiles.map((entry) => entry.path),
        },
        null,
        2,
      )}\n`,
    );
  } finally {
    fs.closeSync(asar.fd);
  }
}

if (command === "offline") {
  generateOfflineEvidence();
  process.exit(0);
}

if (command === "scenario-plan") {
  writeNaturalScenarioPlan();
  process.exit(0);
}

const observerExpression = `(() => {
  const key = ${JSON.stringify(OBSERVER_GLOBAL)};
  const previous = window[key];
  if (previous?.dispose) previous.dispose();

  const state = {
    version: 1,
    installedAt: new Date().toISOString(),
    installedAtPerformanceMs: performance.now(),
    events: [],
    outgoingEvents: [],
    rawChunkEvents: [],
    mutations: [],
    transfers: new Map(),
    sequence: 0,
  };

  const safe = (value, depth = 0, seen = new WeakSet()) => {
    if (value == null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      return typeof value === 'string' && value.length > 500000 ? value.slice(0, 500000) : value;
    }
    if (typeof value === 'bigint') return String(value);
    if (typeof value === 'function') return '[Function]';
    if (typeof value !== 'object') return String(value);
    if (depth >= 12) return '[MaxDepth]';
    if (seen.has(value)) return '[Circular]';
    seen.add(value);
    if (value instanceof Element) {
      return {
        element: value.tagName,
        id: value.id || null,
        className: typeof value.className === 'string' ? value.className.slice(0, 500) : null,
      };
    }
    if (Array.isArray(value)) return value.slice(0, 10000).map((item) => safe(item, depth + 1, seen));
    const result = {};
    for (const key of Object.keys(value).slice(0, 10000)) {
      try { result[key] = safe(value[key], depth + 1, seen); }
      catch (error) { result[key] = '[Unreadable: ' + String(error) + ']'; }
    }
    return result;
  };

  const relevant = (message) => {
    if (message == null || typeof message !== 'object') return false;
    if (message.type === 'mcp-request') {
      return /item\\/fileChange\\/requestApproval/.test(message.request?.method || '');
    }
    if (message.type === 'mcp-notification') {
      return /^(?:turn\\/(?:started|completed|diff\\/updated)|item\\/(?:started|completed)|item\\/fileChange\\/(?:patchUpdated|outputDelta)|serverRequest\\/resolved)$/.test(message.method || '');
    }
    if (message.type === 'mcp-response') return true;
    return false;
  };

  const record = (direction, message, metadata = {}) => {
    if (!relevant(message)) return;
    state.events.push({
      sequence: ++state.sequence,
      capturedAt: new Date().toISOString(),
      performanceMs: performance.now(),
      direction,
      metadata: safe(metadata),
      message: safe(message),
    });
    if (state.events.length > 10000) state.events.splice(0, 2000);
  };

  const unset = Symbol('unset');
  const newAssembler = () => ({ stack: [], root: unset, stringChunks: null, stringTarget: null });
  const saveValue = (assembler, value) => {
    const container = assembler.stack.at(-1);
    if (container == null) {
      if (assembler.root !== unset) throw new Error('multiple roots');
      assembler.root = value;
      return;
    }
    if (container.type === 'array') {
      container.value.push(value);
      return;
    }
    if (container.key == null) throw new Error('object value without key');
    container.value[container.key] = value;
    container.key = null;
  };
  const setKey = (assembler, value) => {
    const container = assembler.stack.at(-1);
    if (container?.type !== 'object' || container.key != null) throw new Error('key outside object');
    container.key = value;
  };
  const consume = (assembler, tokens) => {
    for (const token of tokens) {
      switch (token.type) {
        case 'array-start': {
          const value = [];
          saveValue(assembler, value);
          assembler.stack.push({ type: 'array', value });
          break;
        }
        case 'object-start': {
          const value = {};
          saveValue(assembler, value);
          assembler.stack.push({ type: 'object', value, key: null });
          break;
        }
        case 'container-end':
          if (assembler.stack.pop() == null) throw new Error('unmatched container end');
          break;
        case 'key': setKey(assembler, token.value); break;
        case 'value': saveValue(assembler, token.value); break;
        case 'string-start':
          assembler.stringChunks = [];
          assembler.stringTarget = token.target;
          break;
        case 'string-chunk': assembler.stringChunks.push(token.value); break;
        case 'string-end': {
          const value = assembler.stringChunks.join('');
          const target = assembler.stringTarget;
          assembler.stringChunks = null;
          assembler.stringTarget = null;
          if (target === 'key') setKey(assembler, value); else saveValue(assembler, value);
          break;
        }
      }
    }
  };

  const receive = (data) => {
    if (data?.marker !== 'codex-host-chunked-message-v1') return data;
    state.rawChunkEvents.push({
      capturedAt: new Date().toISOString(),
      performanceMs: performance.now(),
      transferId: data.transferId,
      sequence: data.sequence,
      kind: data.kind,
      tokenCount: Array.isArray(data.tokens) ? data.tokens.length : 0,
    });
    if (data.kind === 'start') {
      state.transfers.clear();
      state.transfers.set(data.transferId, { assembler: newAssembler(), nextSequence: data.sequence + 1 });
      return null;
    }
    const transfer = state.transfers.get(data.transferId);
    if (!transfer || data.sequence !== transfer.nextSequence) {
      state.transfers.delete(data.transferId);
      return null;
    }
    transfer.nextSequence += 1;
    if (data.kind === 'chunk') {
      consume(transfer.assembler, data.tokens || []);
      return null;
    }
    state.transfers.delete(data.transferId);
    return transfer.assembler.root === unset ? null : transfer.assembler.root;
  };

  const incoming = (event) => {
    try {
      const message = receive(event.data);
      if (message != null) {
        record('host_to_renderer', message, {
          origin: event.origin,
          sourceIsWindow: event.source === window,
          portCount: event.ports?.length ?? 0,
        });
      }
    } catch (error) {
      state.events.push({
        sequence: ++state.sequence,
        capturedAt: new Date().toISOString(),
        direction: 'observer_error',
        message: String(error?.stack || error),
      });
    }
  };
  const outgoing = (event) => {
    state.outgoingEvents.push({
      capturedAt: new Date().toISOString(),
      performanceMs: performance.now(),
      forwardedViaBridge: event.__codexForwardedViaBridge === true,
      detail: safe(event.detail),
    });
    if (state.outgoingEvents.length > 2000) state.outgoingEvents.splice(0, 500);
    record('renderer_to_host', event.detail, {
      forwardedViaBridge: event.__codexForwardedViaBridge === true,
    });
  };

  const selectorPath = (element) => {
    if (!(element instanceof Element)) return null;
    const parts = [];
    for (let current = element; current && parts.length < 10; current = current.parentElement) {
      if (current.id) { parts.unshift('#' + CSS.escape(current.id)); break; }
      const parent = current.parentElement;
      const same = parent ? [...parent.children].filter((child) => child.tagName === current.tagName) : [];
      const suffix = same.length > 1 ? ':nth-of-type(' + (same.indexOf(current) + 1) + ')' : '';
      parts.unshift(current.tagName.toLowerCase() + suffix);
    }
    return parts.join(' > ');
  };
  const summarizeNode = (node) => {
    const element = node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
    if (!(element instanceof Element)) return null;
    const rect = element.getBoundingClientRect();
    return {
      selectorPath: selectorPath(element),
      tag: element.tagName,
      attributes: Object.fromEntries([...element.attributes].map((attribute) => [attribute.name, attribute.value])),
      text: (element.innerText || element.textContent || '').trim().slice(0, 4000),
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    };
  };
  const mutationObserver = new MutationObserver((records) => {
    for (const mutation of records) {
      state.mutations.push({
        capturedAt: new Date().toISOString(),
        performanceMs: performance.now(),
        type: mutation.type,
        attributeName: mutation.attributeName,
        oldValue: mutation.oldValue,
        target: summarizeNode(mutation.target),
        added: [...mutation.addedNodes].slice(0, 30).map(summarizeNode),
        removed: [...mutation.removedNodes].slice(0, 30).map(summarizeNode),
      });
    }
    if (state.mutations.length > 10000) state.mutations.splice(0, 3000);
  });

  window.addEventListener('message', incoming, true);
  window.addEventListener('codex-message-from-view', outgoing, true);
  mutationObserver.observe(document.documentElement, {
    subtree: true,
    childList: true,
    characterData: true,
    attributes: true,
    attributeOldValue: true,
  });
  state.dispose = () => {
    window.removeEventListener('message', incoming, true);
    window.removeEventListener('codex-message-from-view', outgoing, true);
    mutationObserver.disconnect();
  };
  state.export = () => ({
    version: state.version,
    installedAt: state.installedAt,
    installedAtPerformanceMs: state.installedAtPerformanceMs,
    exportedAt: new Date().toISOString(),
    events: safe(state.events),
    outgoingEvents: safe(state.outgoingEvents),
    rawChunkEvents: safe(state.rawChunkEvents),
    mutations: safe(state.mutations),
    activeTransfers: state.transfers.size,
  });
  state.clear = () => {
    state.events.length = 0;
    state.outgoingEvents.length = 0;
    state.rawChunkEvents.length = 0;
    state.mutations.length = 0;
    state.transfers.clear();
    state.sequence = 0;
  };
  window[key] = state;
  return {
    installedAt: state.installedAt,
    key,
    bridgeKeys: Object.keys(window.electronBridge || {}),
    appSessionId: window.electronBridge?.getAppSessionId?.() ?? null,
    desktopUserAgent: window.electronBridge?.getDesktopUserAgent?.() ?? null,
    buildFlavor: window.electronBridge?.getBuildFlavor?.() ?? null,
  };
})()`;

const styleProperties = [
  "display",
  "visibility",
  "position",
  "inset",
  "top",
  "right",
  "bottom",
  "left",
  "z-index",
  "box-sizing",
  "width",
  "height",
  "min-width",
  "min-height",
  "max-width",
  "max-height",
  "padding-top",
  "padding-right",
  "padding-bottom",
  "padding-left",
  "margin-top",
  "margin-right",
  "margin-bottom",
  "margin-left",
  "border-top-width",
  "border-right-width",
  "border-bottom-width",
  "border-left-width",
  "border-top-color",
  "border-right-color",
  "border-bottom-color",
  "border-left-color",
  "border-radius",
  "background",
  "background-color",
  "box-shadow",
  "color",
  "opacity",
  "font-family",
  "font-size",
  "font-weight",
  "font-style",
  "line-height",
  "letter-spacing",
  "text-align",
  "text-overflow",
  "text-decoration",
  "white-space",
  "word-break",
  "overflow-x",
  "overflow-y",
  "overscroll-behavior",
  "scrollbar-width",
  "flex-direction",
  "flex-wrap",
  "flex-grow",
  "flex-shrink",
  "align-items",
  "align-self",
  "justify-content",
  "gap",
  "column-gap",
  "row-gap",
  "grid-template-columns",
  "grid-template-rows",
  "cursor",
  "pointer-events",
  "outline",
  "outline-offset",
  "transform",
  "transition",
  "filter",
  "backdrop-filter",
  "color-scheme",
];

const snapshotExpression = `(() => {
  const observer = window[${JSON.stringify(OBSERVER_GLOBAL)}];
  const rect = (element) => {
    const value = element.getBoundingClientRect();
    return {
      x: value.x, y: value.y, width: value.width, height: value.height,
      top: value.top, right: value.right, bottom: value.bottom, left: value.left,
    };
  };
  const attrs = (element) => Object.fromEntries(
    [...element.attributes].map((attribute) => [attribute.name, attribute.value]),
  );
  const properties = ${JSON.stringify(styleProperties)};
  const style = (element, pseudo = null) => {
    const computed = getComputedStyle(element, pseudo);
    return Object.fromEntries(properties.map((property) => [property, computed.getPropertyValue(property)]));
  };
  const selectorPath = (element) => {
    if (!(element instanceof Element)) return null;
    const parts = [];
    for (let current = element; current && parts.length < 20;) {
      if (current.id) { parts.unshift('#' + CSS.escape(current.id)); break; }
      const parent = current.parentElement;
      const currentRoot = current.getRootNode();
      const siblingContainer = parent ?? (currentRoot instanceof ShadowRoot ? currentRoot : null);
      const same = siblingContainer
        ? [...siblingContainer.children].filter((child) => child.tagName === current.tagName)
        : [];
      const suffix = same.length > 1 ? ':nth-of-type(' + (same.indexOf(current) + 1) + ')' : '';
      parts.unshift(current.tagName.toLowerCase() + suffix);
      if (parent) {
        current = parent;
        continue;
      }
      const root = current.getRootNode();
      if (root instanceof ShadowRoot) {
        parts.unshift('::shadow');
        current = root.host;
        continue;
      }
      current = null;
    }
    return parts.join(' > ').replace(/ > ::shadow > /g, ' >>> ');
  };
  const collectDeep = (root) => {
    const result = [];
    const visit = (container) => {
      for (const child of container.children || []) {
        result.push(child);
        if (child.shadowRoot) visit(child.shadowRoot);
        visit(child);
      }
    };
    visit(root);
    return result;
  };
  const deepWithin = (root) => [root, ...collectDeep(root)];
  const directText = (element) => [...element.childNodes]
    .filter((node) => node.nodeType === Node.TEXT_NODE)
    .map((node) => node.textContent || '').join('').trim();
  const stateFor = (element) => ({
    hover: element.matches(':hover'),
    focus: element.matches(':focus'),
    focusVisible: element.matches(':focus-visible'),
    focusWithin: element.matches(':focus-within'),
    active: element.matches(':active'),
    disabled: element.matches(':disabled'),
    checked: element.matches(':checked'),
    expanded: element.getAttribute('aria-expanded'),
    selected: element.getAttribute('aria-selected'),
    pressed: element.getAttribute('aria-pressed'),
  });
  const reactData = (element) => {
    const result = [];
    for (let current = element, depth = 0; current && depth < 10; current = current.parentElement, depth += 1) {
      const propsKey = Object.keys(current).find((key) => key.startsWith('__reactProps$'));
      const fiberKey = Object.keys(current).find((key) => key.startsWith('__reactFiber$'));
      const safe = (value, level = 0, seen = new WeakSet()) => {
        if (value == null || ['string','number','boolean'].includes(typeof value)) {
          return typeof value === 'string' ? value.slice(0, 100000) : value;
        }
        if (typeof value === 'function') return '[Function]';
        if (typeof value !== 'object') return String(value);
        if (level >= 8) return '[MaxDepth]';
        if (seen.has(value)) return '[Circular]';
        if (value instanceof Element) return '[Element ' + value.tagName + ']';
        seen.add(value);
        if (Array.isArray(value)) return value.slice(0, 500).map((item) => safe(item, level + 1, seen));
        const output = {};
        for (const key of Object.keys(value).slice(0, 500)) {
          if (['return','child','sibling','stateNode','alternate','_owner'].includes(key)) continue;
          try { output[key] = safe(value[key], level + 1, seen); }
          catch { output[key] = '[Unreadable]'; }
        }
        return output;
      };
      if (propsKey || fiberKey) {
        const fiber = fiberKey ? current[fiberKey] : null;
        result.push({
          ancestorDepth: depth,
          selectorPath: selectorPath(current),
          props: propsKey ? safe(current[propsKey]) : null,
          fiber: fiber ? {
            elementType: typeof fiber.elementType === 'string' ? fiber.elementType : fiber.elementType?.name || null,
            type: typeof fiber.type === 'string' ? fiber.type : fiber.type?.name || null,
            memoizedProps: safe(fiber.memoizedProps),
            memoizedState: safe(fiber.memoizedState),
            pendingProps: safe(fiber.pendingProps),
          } : null,
        });
      }
    }
    return result;
  };
  const node = (element, includeReact = false) => ({
    selectorPath: selectorPath(element),
    tag: element.tagName,
    attributes: attrs(element),
    directText: directText(element),
    text: (element.innerText || element.textContent || '').trim().slice(0, 250000),
    rect: rect(element),
    clientRects: [...element.getClientRects()].map((value) => ({
      x: value.x, y: value.y, width: value.width, height: value.height,
    })),
    box: {
      clientWidth: element.clientWidth, clientHeight: element.clientHeight,
      offsetWidth: element.offsetWidth, offsetHeight: element.offsetHeight,
      scrollWidth: element.scrollWidth, scrollHeight: element.scrollHeight,
      scrollLeft: element.scrollLeft, scrollTop: element.scrollTop,
    },
    state: stateFor(element),
    style: style(element),
    before: style(element, '::before'),
    after: style(element, '::after'),
    root: (() => {
      const root = element.getRootNode();
      return root instanceof ShadowRoot ? {
        kind: 'shadow',
        mode: root.mode,
        delegatesFocus: root.delegatesFocus,
        hostSelectorPath: selectorPath(root.host),
      } : { kind: root === document ? 'document' : root?.constructor?.name ?? null };
    })(),
    shadowRoot: element.shadowRoot ? {
      mode: element.shadowRoot.mode,
      delegatesFocus: element.shadowRoot.delegatesFocus,
      childElementCount: element.shadowRoot.childElementCount,
    } : null,
    assignedSlotSelectorPath: element.assignedSlot ? selectorPath(element.assignedSlot) : null,
    react: includeReact ? reactData(element) : undefined,
  });
  const visible = (element) => {
    const bounds = element.getBoundingClientRect();
    const computed = getComputedStyle(element);
    return bounds.width > 0 && bounds.height > 0 && computed.display !== 'none' && computed.visibility !== 'hidden';
  };
  const all = collectDeep(document);
  const approvalRoots = all.filter((element) =>
    visible(element) && (
      element.hasAttribute('data-codex-approval-surface') ||
      /file.*approval|approval.*file/i.test([
        element.getAttribute('data-testid'), element.getAttribute('aria-label'),
        element.getAttribute('class'), element.getAttribute('data-state'),
      ].filter(Boolean).join(' '))
    )
  );
  const reviewFileRoots = all.filter((element) =>
    visible(element) && element.hasAttribute('data-review-path')
  );
  const reviewScrollRoots = all.filter((element) =>
    visible(element) && element.hasAttribute('data-app-action-review-scroll')
  );
  const genericReviewRoots = all.filter((element) =>
    visible(element) && (
      element.hasAttribute('data-app-action-review-file-expanded') ||
      element.hasAttribute('data-app-action-review-file-toggle') ||
      /diff|review-file|changed-file/i.test([
        element.getAttribute('data-testid'), element.getAttribute('aria-label'),
        element.getAttribute('class'), element.getAttribute('data-state'),
      ].filter(Boolean).join(' '))
    )
  );
  const dedupeRoots = (roots) => roots.filter((root) => !roots.some((other) => other !== root && other.contains(root)));
  const surfaceSpecs = [
    ...dedupeRoots(approvalRoots).map((root) => ({ kind: 'approval', root })),
    ...reviewScrollRoots.map((root) => ({ kind: 'review-scroll', root })),
    ...reviewFileRoots.map((root) => ({ kind: 'review-file', root })),
  ];
  if (reviewScrollRoots.length === 0 && reviewFileRoots.length === 0) {
    surfaceSpecs.push(
      ...dedupeRoots(genericReviewRoots).map((root) => ({ kind: 'review-fallback', root })),
    );
  }
  const surfaceTrees = surfaceSpecs.map(({ kind, root }) => ({
    kind,
    root: node(root, true),
    descendants: deepWithin(root).map((element) => node(element)),
    outerHTML: root.outerHTML,
  }));
  const controls = all.filter((element) =>
    visible(element) && element.matches('button,input,textarea,select,[contenteditable="true"],[role="button"],[role="menuitem"],[role="option"],[role="radio"],[role="checkbox"],[tabindex]')
  ).map((element) => node(element, true));
  const textEvidence = all.filter((element) => {
    if (!visible(element)) return false;
    const text = (element.innerText || element.textContent || '').trim();
    const metadata = Object.values(attrs(element)).join(' ');
    return /文件|更改|变更|审批|批准|允许|拒绝|审查|复制|打开|file|change|diff|review|copy|open|added|deleted|modified/i.test(text + ' ' + metadata);
  }).sort((a, b) => {
    const ar = a.getBoundingClientRect(), br = b.getBoundingClientRect();
    return (ar.width * ar.height) - (br.width * br.height);
  }).slice(0, 1200).map((element) => node(element));
  const scrollContainers = all.filter((element) => {
    const computed = getComputedStyle(element);
    return visible(element) && (
      element.scrollHeight > element.clientHeight || element.scrollWidth > element.clientWidth ||
      /auto|scroll/.test(computed.overflowX + ' ' + computed.overflowY)
    );
  }).map((element) => node(element));
  const reviewFiles = reviewFileRoots.map((root) => {
    const descendants = deepWithin(root);
    const diffLike = descendants.filter((element) => {
      if (!visible(element)) return false;
      const metadata = [
        element.tagName,
        element.getAttribute('class'),
        element.getAttribute('part'),
        ...[...element.attributes].flatMap((attribute) => [attribute.name, attribute.value]),
      ].filter(Boolean).join(' ');
      return /diff|hunk|line|gutter|addition|deletion|insert|remove|context|code/i.test(metadata);
    });
    const leafText = descendants.filter((element) => {
      if (!visible(element)) return false;
      const text = (element.innerText || element.textContent || '').trim();
      return text.length > 0 && collectDeep(element).every((child) => {
        const childText = (child.innerText || child.textContent || '').trim();
        return childText.length === 0;
      });
    });
    return {
      path: root.getAttribute('data-review-path'),
      root: node(root, true),
      toggle: descendants.find((element) => element.hasAttribute('data-app-action-review-file-toggle'))
        ? node(descendants.find((element) => element.hasAttribute('data-app-action-review-file-toggle')), true)
        : null,
      descendants: descendants.map((element) => node(element)),
      diffLikeNodes: diffLike.map((element) => node(element)),
      leafTextNodes: leafText.map((element) => node(element)),
    };
  });
  const shadowDOM = all.filter((element) => element.shadowRoot).map((host) => ({
    host: node(host),
    innerHTML: host.shadowRoot.innerHTML,
    descendants: collectDeep(host.shadowRoot).map((element) => node(element)),
  }));
  return {
    capturedAt: new Date().toISOString(),
    location: location.href,
    title: document.title,
    navigator: { userAgent: navigator.userAgent, language: navigator.language, platform: navigator.platform },
    viewport: {
      innerWidth, innerHeight, outerWidth, outerHeight, devicePixelRatio,
      scrollX, scrollY,
      visualViewport: visualViewport ? {
        width: visualViewport.width, height: visualViewport.height,
        offsetLeft: visualViewport.offsetLeft, offsetTop: visualViewport.offsetTop,
        pageLeft: visualViewport.pageLeft, pageTop: visualViewport.pageTop,
        scale: visualViewport.scale,
      } : null,
    },
    theme: {
      htmlClass: document.documentElement.className,
      bodyClass: document.body.className,
      colorScheme: getComputedStyle(document.documentElement).colorScheme,
      htmlBackground: getComputedStyle(document.documentElement).backgroundColor,
      bodyBackground: getComputedStyle(document.body).backgroundColor,
      bodyColor: getComputedStyle(document.body).color,
    },
    documentScroll: {
      scrollingElement: document.scrollingElement ? node(document.scrollingElement) : null,
      body: node(document.body),
      documentElement: node(document.documentElement),
    },
    activeElement: document.activeElement instanceof Element ? node(document.activeElement, true) : null,
    hoveredElements: all.filter((element) => element.matches(':hover')).map((element) => node(element)),
    focusVisibleElements: all.filter((element) => element.matches(':focus-visible')).map((element) => node(element)),
    controls,
    surfaceTrees,
    reviewFiles,
    shadowDOM,
    textEvidence,
    scrollContainers,
    fullDocumentHTML: document.documentElement.outerHTML,
    observer: observer?.export?.() ?? null,
  };
})()`;

if (command === "self-test") {
  const compile = (name, expression) => {
    try {
      new Function(`return (${expression});`);
      return { name, parsed: true, length: expression.length };
    } catch (error) {
      throw new Error(`${name} browser expression failed to parse: ${String(error)}`);
    }
  };
  const plan = naturalScenarioPlan();
  const patchCount = (prompt, marker) => prompt.split(marker).length - 1;
  const result = {
    browserExpressions: [
      compile("observerExpression", observerExpression),
      compile("snapshotExpression", snapshotExpression),
    ],
    scenario: {
      fixturePaths: plan.fixturePaths,
      createFileSections: patchCount(plan.prompt1, "*** Add File:"),
      updateFileSections: patchCount(plan.prompt2, "*** Update File:"),
      firstApproval: plan.expected.firstApproval,
      secondApproval: plan.expected.secondApproval,
      finalLineCounts: Object.fromEntries(
        Object.entries(plan.expected.finalContents).map(([filePath, contents]) => [
          filePath,
          contents.trimEnd().split("\n").length,
        ]),
      ),
    },
  };
  if (result.scenario.createFileSections !== 2 || result.scenario.updateFileSections !== 2) {
    throw new Error("Natural scenario no longer contains exactly two files per patch");
  }
  if (Object.values(result.scenario.finalLineCounts).some((count) => count !== 48)) {
    throw new Error("Natural scenario final line-count invariant failed");
  }
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  process.exit(0);
}

ensureArtifactDir();

const targets = await (await fetch(`${CDP_HTTP}/json/list`)).json();
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.title === "ChatGPT" &&
    candidate.url === "app://-/index.html",
);
if (!target) throw new Error("ChatGPT app://-/index.html target not found on CDP port 9222");

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 0;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id == null) return;
  const callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  if (message.error) callback.reject(new Error(JSON.stringify(message.error)));
  else callback.resolve(message.result);
};

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
    );
  }
  return result.result.value;
}

await Promise.all([
  send("Page.enable"),
  send("Runtime.enable"),
  send("DOM.enable"),
  send("DOMSnapshot.enable"),
  send("Accessibility.enable"),
  send("Performance.enable"),
]);

async function pointFor(selector, index = 0) {
  return evaluate(`(() => {
    const selector = ${JSON.stringify(selector)};
    const index = ${Number(index)};
    const elements = [...document.querySelectorAll(selector)].filter((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
    });
    const element = elements[index];
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, rect: {
      x: rect.x, y: rect.y, width: rect.width, height: rect.height,
    }, count: elements.length };
  })()`);
}

async function pointForText(text, selector = "*") {
  return evaluate(`(() => {
    const text = ${JSON.stringify(text)};
    const selector = ${JSON.stringify(selector)};
    const elements = [...document.querySelectorAll(selector)].filter((element) => {
      const rect = element.getBoundingClientRect();
      const value = (element.innerText || element.textContent || '').trim();
      return rect.width > 0 && rect.height > 0 && value === text;
    }).sort((a, b) => {
      const ar = a.getBoundingClientRect(), br = b.getBoundingClientRect();
      return (ar.width * ar.height) - (br.width * br.height);
    });
    const element = elements[0];
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2, rect: {
      x: rect.x, y: rect.y, width: rect.width, height: rect.height,
    }, count: elements.length };
  })()`);
}

async function dispatchClick(point) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

function keyParameters(key, modifier = null) {
  const byKey = {
    Escape: { code: "Escape", keyCode: 27 },
    Enter: { code: "Enter", keyCode: 13 },
    Tab: { code: "Tab", keyCode: 9 },
    ArrowDown: { code: "ArrowDown", keyCode: 40 },
    ArrowUp: { code: "ArrowUp", keyCode: 38 },
    ArrowLeft: { code: "ArrowLeft", keyCode: 37 },
    ArrowRight: { code: "ArrowRight", keyCode: 39 },
    Space: { code: "Space", keyCode: 32, key: " " },
  };
  const result = byKey[key] ?? { code: key, keyCode: 0 };
  return {
    key: result.key ?? key,
    code: result.code,
    windowsVirtualKeyCode: result.keyCode,
    nativeVirtualKeyCode: result.keyCode,
    modifiers: modifier === "shift" ? 8 : 0,
  };
}

async function capture(name) {
  if (!name || !/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(name)) {
    throw new Error("Capture name must contain only letters, digits, dots, underscores, and hyphens");
  }
  const [snapshot, domSnapshot, axTree, browserVersion, metrics] = await Promise.all([
    evaluate(snapshotExpression),
    send("DOMSnapshot.captureSnapshot", {
      computedStyles: styleProperties,
      includeDOMRects: true,
      includePaintOrder: true,
      includeBlendedBackgroundColors: true,
      includeTextColorOpacities: true,
    }),
    send("Accessibility.getFullAXTree"),
    send("Browser.getVersion"),
    send("Performance.getMetrics").catch(() => null),
  ]);
  snapshot.cdp = {
    endpoint: CDP_HTTP,
    target: { id: target.id, title: target.title, type: target.type, url: target.url },
    browserVersion,
    metrics,
  };

  const prefix = path.join(artifactDir, name);
  fs.writeFileSync(`${prefix}.snapshot.json`, `${JSON.stringify(snapshot, null, 2)}\n`);
  fs.writeFileSync(`${prefix}.domsnapshot.json`, `${JSON.stringify(domSnapshot, null, 2)}\n`);
  fs.writeFileSync(`${prefix}.axtree.json`, `${JSON.stringify(axTree, null, 2)}\n`);

  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(`${prefix}.png`, Buffer.from(screenshot.data, "base64"));

  const surfaceRects = snapshot.surfaceTrees
    .map((surface) => surface.root.rect)
    .filter(
      (rect) =>
        rect.width > 0 &&
        rect.height > 0 &&
        rect.x >= 0 &&
        rect.y >= 0 &&
        rect.right <= snapshot.viewport.innerWidth &&
        rect.bottom <= snapshot.viewport.innerHeight,
    );
  for (const [index, rect] of surfaceRects.entries()) {
    const clip = await send("Page.captureScreenshot", {
      format: "png",
      fromSurface: true,
      captureBeyondViewport: false,
      clip: {
        x: Math.floor(rect.x),
        y: Math.floor(rect.y),
        width: Math.ceil(rect.width),
        height: Math.ceil(rect.height),
        scale: 1,
      },
    });
    fs.writeFileSync(
      `${prefix}.surface-${String(index + 1).padStart(2, "0")}.png`,
      Buffer.from(clip.data, "base64"),
    );
  }

  return {
    name,
    prefix,
    theme: snapshot.theme,
    surfaceCount: snapshot.surfaceTrees.length,
    reviewFileCount: snapshot.reviewFiles.length,
    shadowRootCount: snapshot.shadowDOM.length,
    controlCount: snapshot.controls.length,
    textEvidenceCount: snapshot.textEvidence.length,
    scrollContainerCount: snapshot.scrollContainers.length,
    protocolEventCount: snapshot.observer?.events?.length ?? null,
    mutationCount: snapshot.observer?.mutations?.length ?? null,
  };
}

let result;
switch (command) {
  case "submit-scenario": {
    const step = Number(positional[0]);
    if (step !== 1 && step !== 2) throw new Error("submit-scenario step must be 1 or 2");
    const scenario = naturalScenarioPlan();
    const prompt = step === 1 ? scenario.prompt1 : scenario.prompt2;
    const prepared = await evaluate(`(() => {
      const permission = [...document.querySelectorAll('button')]
        .find((element) => element.getAttribute('aria-label') === '更改权限');
      const composer = [...document.querySelectorAll('[contenteditable="true"]')]
        .find((element) => {
          const rect = element.getBoundingClientRect();
          return rect.width > 0 && rect.height > 0;
        });
      if (!composer) return { ok: false, reason: 'visible-composer-not-found' };
      const existingText = (composer.innerText || composer.textContent || '').trim();
      if (existingText.length > 0) {
        return { ok: false, reason: 'composer-not-empty', existingText };
      }
      const permissionText = (permission?.innerText || permission?.textContent || '').trim();
      if (permissionText !== '请求批准') {
        return { ok: false, reason: 'permission-mode-not-request', permissionText };
      }
      composer.focus();
      return {
        ok: document.activeElement === composer,
        permissionText,
        composerTag: composer.tagName,
        composerRect: composer.getBoundingClientRect().toJSON(),
      };
    })()`);
    if (!prepared?.ok) throw new Error(`Scenario composer preflight failed: ${JSON.stringify(prepared)}`);
    await send("Input.insertText", { text: prompt });
    const inserted = await evaluate(`(() => {
      const composer = document.activeElement;
      return {
        tag: composer?.tagName ?? null,
        textLength: (composer?.innerText || composer?.textContent || '').length,
        textStart: (composer?.innerText || composer?.textContent || '').slice(0, 120),
        textEnd: (composer?.innerText || composer?.textContent || '').slice(-120),
      };
    })()`);
    const enter = keyParameters("Enter");
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...enter });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...enter });
    await new Promise((resolve) => setTimeout(resolve, 350));
    result = { step, promptLength: prompt.length, prepared, inserted };
    break;
  }
  case "submit-approval-retry":
  case "submit-external-approval":
  case "submit-many-external-approval": {
    const fixturePaths =
      command === "submit-many-external-approval"
        ? MANY_EXTERNAL_APPROVAL_PATHS
        : command === "submit-external-approval"
          ? EXTERNAL_APPROVAL_PATHS
          : APPROVAL_RETRY_PATHS;
    const prompt =
      command === "submit-many-external-approval"
        ? manyExternalApprovalPrompt()
        : approvalRetryPrompt(fixturePaths);
    const prepared = await evaluate(`(() => {
      const permission = [...document.querySelectorAll('button')]
        .find((element) => element.getAttribute('aria-label') === '更改权限');
      const composer = [...document.querySelectorAll('[contenteditable="true"]')]
        .find((element) => {
          const rect = element.getBoundingClientRect();
          return rect.width > 0 && rect.height > 0;
        });
      if (!composer) return { ok: false, reason: 'visible-composer-not-found' };
      const existingText = (composer.innerText || composer.textContent || '').trim();
      if (existingText.length > 0) {
        return { ok: false, reason: 'composer-not-empty', existingText };
      }
      const permissionText = (permission?.innerText || permission?.textContent || '').trim();
      if (permissionText !== '请求批准') {
        return { ok: false, reason: 'permission-mode-not-request', permissionText };
      }
      composer.focus();
      return {
        ok: document.activeElement === composer,
        permissionText,
        composerTag: composer.tagName,
        composerRect: composer.getBoundingClientRect().toJSON(),
      };
    })()`);
    if (!prepared?.ok) {
      throw new Error(`Approval retry composer preflight failed: ${JSON.stringify(prepared)}`);
    }
    await send("Input.insertText", { text: prompt });
    const inserted = await evaluate(`(() => {
      const composer = document.activeElement;
      return {
        tag: composer?.tagName ?? null,
        textLength: (composer?.innerText || composer?.textContent || '').length,
        textStart: (composer?.innerText || composer?.textContent || '').slice(0, 120),
        textEnd: (composer?.innerText || composer?.textContent || '').slice(-120),
      };
    })()`);
    const enter = keyParameters("Enter");
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...enter });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...enter });
    await new Promise((resolve) => setTimeout(resolve, 350));
    result = {
      fixturePaths,
      promptLength: prompt.length,
      prepared,
      inserted,
    };
    break;
  }
  case "install":
    result = await evaluate(observerExpression);
    break;
  case "observer-status":
    result = await evaluate(`(() => {
      const value = window[${JSON.stringify(OBSERVER_GLOBAL)}];
      return value ? {
        installedAt: value.installedAt,
        eventCount: value.events.length,
        outgoingEventCount: value.outgoingEvents?.length ?? null,
        rawChunkEventCount: value.rawChunkEvents.length,
        mutationCount: value.mutations.length,
        activeTransfers: value.transfers.size,
      } : null;
    })()`);
    break;
  case "clear-events":
    result = await evaluate(`(() => {
      const value = window[${JSON.stringify(OBSERVER_GLOBAL)}];
      if (!value) return { cleared: false, reason: 'observer-not-installed' };
      value.clear();
      return { cleared: true, at: new Date().toISOString() };
    })()`);
    break;
  case "events": {
    const name = positional[0] ?? "protocol-events";
    const events = await evaluate(
      `window[${JSON.stringify(OBSERVER_GLOBAL)}]?.export?.() ?? null`,
    );
    const output = path.join(artifactDir, `${name}.json`);
    fs.writeFileSync(output, `${JSON.stringify(events, null, 2)}\n`);
    result = {
      output,
      eventCount: events?.events?.length ?? null,
      rawChunkEventCount: events?.rawChunkEvents?.length ?? null,
      mutationCount: events?.mutations?.length ?? null,
    };
    break;
  }
  case "capture":
    result = await capture(positional[0]);
    break;
  case "click": {
    const selector = positional[0];
    const index = Number(positional[1] ?? 0);
    const point = await pointFor(selector, index);
    if (!point) throw new Error(`Visible selector not found: ${selector} index=${index}`);
    await dispatchClick(point);
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { selector, index, point };
    break;
  }
  case "click-text": {
    const text = positional[0];
    const selector = positional[1] ?? "*";
    const point = await pointForText(text, selector);
    if (!point) throw new Error(`Visible text not found: ${text} selector=${selector}`);
    await dispatchClick(point);
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { text, selector, point };
    break;
  }
  case "hover": {
    const selector = positional[0];
    const index = Number(positional[1] ?? 0);
    const point = await pointFor(selector, index);
    if (!point) throw new Error(`Visible selector not found: ${selector} index=${index}`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { selector, index, point };
    break;
  }
  case "hover-text": {
    const text = positional[0];
    const selector = positional[1] ?? "*";
    const point = await pointForText(text, selector);
    if (!point) throw new Error(`Visible text not found: ${text} selector=${selector}`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { text, selector, point };
    break;
  }
  case "focus": {
    const selector = positional[0];
    const index = Number(positional[1] ?? 0);
    result = await evaluate(`(() => {
      const elements = [...document.querySelectorAll(${JSON.stringify(selector)})];
      const element = elements[${index}];
      if (!element) return null;
      element.focus({ preventScroll: true });
      return { active: document.activeElement === element, tag: element.tagName };
    })()`);
    if (!result) throw new Error(`Selector not found: ${selector} index=${index}`);
    break;
  }
  case "press": {
    const key = positional[0];
    const modifier = positional[1] ?? null;
    const params = keyParameters(key, modifier);
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...params });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...params });
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { key, modifier, params };
    break;
  }
  case "press-capture": {
    const key = positional[0];
    const name = positional[1];
    const modifier = positional[2] ?? null;
    const params = keyParameters(key, modifier);
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...params });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...params });
    result = { key, modifier, params, capture: await capture(name) };
    break;
  }
  case "insert": {
    const text = positional.join(" ");
    await send("Input.insertText", { text });
    await new Promise((resolve) => setTimeout(resolve, 250));
    result = { insertedLength: text.length };
    break;
  }
  case "scroll": {
    const selector = positional[0];
    const top = Number(positional[1] ?? 0);
    const left = Number(positional[2] ?? 0);
    result = await evaluate(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return null;
      element.scrollTo({ top: ${top}, left: ${left}, behavior: 'instant' });
      return {
        scrollTop: element.scrollTop, scrollLeft: element.scrollLeft,
        scrollWidth: element.scrollWidth, scrollHeight: element.scrollHeight,
        clientWidth: element.clientWidth, clientHeight: element.clientHeight,
      };
    })()`);
    if (!result) throw new Error(`Selector not found: ${selector}`);
    await new Promise((resolve) => setTimeout(resolve, 250));
    break;
  }
  case "evaluate":
    result = await evaluate(positional.join(" "));
    break;
  default:
    throw new Error(`Unknown command: ${command}\n\n${usage}`);
}

socket.close();
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
