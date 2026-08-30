import fs from "node:fs";
import path from "node:path";

const CDP_HTTP = "http://127.0.0.1:9222";
const OBSERVER_KEY = "__gpuiPermissionProtocolObserver";
const command = process.argv.find((value) => value.startsWith("--command="))
  ?.slice("--command=".length) ?? "inspect";
const mode = process.argv.find((value) => value.startsWith("--mode="))
  ?.slice("--mode=".length) ?? null;
const outputDir = path.resolve(
  process.argv.find((value) => value.startsWith("--artifact-dir="))
    ?.slice("--artifact-dir=".length) ??
    "artifacts/chatgpt-permission-protocol-cdp-2026-08-30",
);
const prompt = process.argv.find((value) => value.startsWith("--prompt="))
  ?.slice("--prompt=".length) ??
  "这是权限协议取证。请只回复 OK，不要调用工具，不要读写文件，不要联网。";

const modeLabels = {
  request: ["请求批准", "Ask for approval"],
  assist: ["帮我批准", "Approve for me"],
  full: ["完全访问权限", "Full access"],
  custom: ["自定义", "Custom"],
};

if (!["inspect", "select-existing", "new-thread", "reload-capture", "export"].includes(command)) {
  throw new Error(`Unsupported command: ${command}`);
}
if (["select-existing", "new-thread"].includes(command) && !(mode in modeLabels)) {
  throw new Error(`--mode must be one of ${Object.keys(modeLabels).join(", ")}`);
}

fs.mkdirSync(outputDir, { recursive: true });

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

async function sleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function clickPoint(x, y) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x,
    y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x,
    y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function clickElement(expression, description) {
  const rect = await evaluate(`(() => {
    const element = (${expression});
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  })()`);
  if (!rect) throw new Error(`${description} not found or not visible`);
  await clickPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
  return rect;
}

async function screenshot(name) {
  const result = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(path.join(outputDir, `${name}.png`), Buffer.from(result.data, "base64"));
}

const observerExpression = `(() => {
  const observerKey = ${JSON.stringify(OBSERVER_KEY)};
  window[observerKey]?.dispose?.();

  const state = {
    version: 1,
    installedAt: new Date().toISOString(),
    installedAtPerformanceMs: performance.now(),
    sequence: 0,
    events: [],
    rawChunkEvents: [],
    transfers: new Map(),
    watchedRequestIds: new Set(),
  };

  const redactedKey = (key) => /(?:token|secret|credential|authorization|api[_-]?key|password)/i.test(key);
  const safe = (value, depth = 0, seen = new WeakSet(), key = '') => {
    if (redactedKey(key)) return '[REDACTED]';
    if (value == null || typeof value === 'number' || typeof value === 'boolean') return value;
    if (typeof value === 'string') return value.length > 200000 ? value.slice(0, 200000) : value;
    if (typeof value === 'bigint') return String(value);
    if (typeof value === 'function') return '[Function]';
    if (typeof value !== 'object') return String(value);
    if (depth >= 16) return '[MaxDepth]';
    if (seen.has(value)) return '[Circular]';
    seen.add(value);
    if (Array.isArray(value)) {
      return value.slice(0, 20000).map((item) => safe(item, depth + 1, seen));
    }
    const output = {};
    for (const property of Object.keys(value).slice(0, 20000)) {
      try { output[property] = safe(value[property], depth + 1, seen, property); }
      catch (error) { output[property] = '[Unreadable: ' + String(error) + ']'; }
    }
    return output;
  };

  const watched = /^(?:initialize|permissionProfile\\/list|config\\/read|configRequirements\\/read|thread\\/(?:start|settings\\/update)|turn\\/start)$/;
  const watchedNotification = /^(?:thread\\/(?:started|settings\\/updated)|turn\\/(?:started|completed))$/;
  const methodOf = (message) => message?.request?.method ?? message?.method ?? null;
  const relevant = (message) => {
    if (message == null || typeof message !== 'object') return false;
    if (message.type === 'mcp-request') {
      const matches = watched.test(methodOf(message) || '');
      if (matches && message.request?.id != null) state.watchedRequestIds.add(message.request.id);
      return matches;
    }
    if (message.type === 'mcp-notification') return watchedNotification.test(message.method || '');
    if (message.type === 'mcp-response') {
      const id = message.message?.id;
      if (id == null || !state.watchedRequestIds.has(id)) return false;
      state.watchedRequestIds.delete(id);
      return true;
    }
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
  };

  const unset = Symbol('unset');
  const newAssembler = () => ({ stack: [], root: unset, stringChunks: null, stringTarget: null });
  const saveValue = (assembler, value) => {
    const container = assembler.stack.at(-1);
    if (container == null) { assembler.root = value; return; }
    if (container.type === 'array') { container.value.push(value); return; }
    container.value[container.key] = value;
    container.key = null;
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
        case 'container-end': assembler.stack.pop(); break;
        case 'key': assembler.stack.at(-1).key = token.value; break;
        case 'value': saveValue(assembler, token.value); break;
        case 'string-start':
          assembler.stringChunks = [];
          assembler.stringTarget = token.target;
          break;
        case 'string-chunk': assembler.stringChunks.push(token.value); break;
        case 'string-end': {
          const value = assembler.stringChunks.join('');
          if (assembler.stringTarget === 'key') assembler.stack.at(-1).key = value;
          else saveValue(assembler, value);
          assembler.stringChunks = null;
          assembler.stringTarget = null;
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
      if (message != null) record('host_to_renderer', message, { origin: event.origin });
    } catch (error) {
      state.events.push({
        sequence: ++state.sequence,
        capturedAt: new Date().toISOString(),
        direction: 'observer_error',
        message: String(error?.stack || error),
      });
    }
  };
  const outgoing = (event) => record('renderer_to_host', event.detail, {
    forwardedViaBridge: event.__codexForwardedViaBridge === true,
  });

  window.addEventListener('message', incoming, true);
  window.addEventListener('codex-message-from-view', outgoing, true);
  state.dispose = () => {
    window.removeEventListener('message', incoming, true);
    window.removeEventListener('codex-message-from-view', outgoing, true);
  };
  state.clear = () => {
    state.sequence = 0;
    state.events.length = 0;
    state.rawChunkEvents.length = 0;
    state.transfers.clear();
    state.watchedRequestIds.clear();
  };
  state.export = () => ({
    version: state.version,
    installedAt: state.installedAt,
    installedAtPerformanceMs: state.installedAtPerformanceMs,
    exportedAt: new Date().toISOString(),
    events: safe(state.events),
    rawChunkEvents: safe(state.rawChunkEvents),
    activeTransfers: state.transfers.size,
  });
  window[observerKey] = state;
  return {
    installedAt: state.installedAt,
    key: observerKey,
    appSessionId: window.electronBridge?.getAppSessionId?.() ?? null,
    buildFlavor: window.electronBridge?.getBuildFlavor?.() ?? null,
  };
})()`;

async function installObserver() {
  return evaluate(observerExpression);
}

async function exportObserver(name, metadata = {}) {
  const trace = await evaluate(`window[${JSON.stringify(OBSERVER_KEY)}]?.export?.() ?? null`);
  const payload = {
    captureVersion: 1,
    name,
    capturedAt: new Date().toISOString(),
    command,
    mode,
    target: {
      id: target.id,
      title: target.title,
      type: target.type,
      url: target.url,
    },
    metadata,
    trace,
  };
  fs.writeFileSync(path.join(outputDir, `${name}.json`), JSON.stringify(payload, null, 2));
  return payload;
}

async function domSnapshot() {
  return evaluate(`(() => {
    const rect = (element) => {
      const value = element.getBoundingClientRect();
      return { x: value.x, y: value.y, width: value.width, height: value.height };
    };
    const describe = (element) => ({
      tag: element.tagName,
      text: (element.innerText || element.textContent || '').trim().slice(0, 4000),
      ariaLabel: element.getAttribute('aria-label'),
      role: element.getAttribute('role'),
      dataState: element.getAttribute('data-state'),
      rect: rect(element),
      outerHTML: element.outerHTML.slice(0, 30000),
    });
    const visible = (element) => {
      const value = element.getBoundingClientRect();
      return value.width > 0 && value.height > 0;
    };
    return {
      capturedAt: new Date().toISOString(),
      href: location.href,
      title: document.title,
      viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
      permissionTrigger: [...document.querySelectorAll('button')]
        .filter(visible)
        .filter((element) => /更改权限|Change permissions/i.test(element.getAttribute('aria-label') || ''))
        .map(describe),
      menuItems: [...document.querySelectorAll('[role="menuitem"], [role="option"], [role="radio"], button')]
        .filter(visible)
        .filter((element) => /请求批准|帮我批准|完全访问权限|自定义|Ask for approval|Approve for me|Full access|Custom/.test((element.innerText || element.textContent || '').trim()))
        .map(describe),
      dialogs: [...document.querySelectorAll('[role="dialog"]')].filter(visible).map(describe),
      composer: [...document.querySelectorAll('[role="textbox"][contenteditable="true"]')].filter(visible).map(describe),
      requestsText: document.body.innerText.includes('请求批准') ? 'permission-text-present' : null,
    };
  })()`);
}

async function saveSnapshot(name, withScreenshot = true) {
  const snapshot = await domSnapshot();
  fs.writeFileSync(path.join(outputDir, `${name}.dom.json`), JSON.stringify(snapshot, null, 2));
  if (withScreenshot) await screenshot(name);
  return snapshot;
}

async function openPermissionMenu() {
  await clickElement(
    `[...document.querySelectorAll('button')].find((element) => /更改权限|Change permissions/i.test(element.getAttribute('aria-label') || ''))`,
    "permission trigger",
  );
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const hasMenu = await evaluate(`(() => [...document.querySelectorAll('[role="menuitem"], [role="option"], [role="radio"], button')]
      .some((element) => {
        const rect = element.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 && /请求批准|帮我批准|完全访问权限|自定义|Ask for approval|Approve for me|Full access|Custom/.test((element.innerText || element.textContent || '').trim());
      }))()`);
    if (hasMenu) return;
    await sleep(50);
  }
  throw new Error("Permission menu did not open");
}

async function selectMode(selectedMode) {
  await openPermissionMenu();
  await saveSnapshot(`${command}-${selectedMode}-menu`);
  const labels = modeLabels[selectedMode];
  await clickElement(
    `[...document.querySelectorAll('[role="menuitem"], [role="option"], [role="radio"], button')]
      .filter((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })
      .find((element) => ${JSON.stringify(labels)}.some((label) => (element.innerText || element.textContent || '').trim().startsWith(label)))`,
    `permission menu item ${selectedMode}`,
  );

  if (selectedMode === "full") {
    const deadline = Date.now() + 5_000;
    while (Date.now() < deadline) {
      const confirmation = await evaluate(`(() => [...document.querySelectorAll('[role="dialog"] button, button')]
        .find((element) => {
          const rect = element.getBoundingClientRect();
          const text = (element.innerText || element.textContent || '').trim();
          return rect.width > 0 && rect.height > 0 && /^(确认|Confirm)$/.test(text);
        }) != null)()`);
      if (confirmation) {
        await saveSnapshot(`${command}-${selectedMode}-confirmation`);
        await clickElement(
          `[...document.querySelectorAll('[role="dialog"] button, button')].find((element) => {
            const rect = element.getBoundingClientRect();
            const text = (element.innerText || element.textContent || '').trim();
            return rect.width > 0 && rect.height > 0 && /^(确认|Confirm)$/.test(text);
          })`,
          "full access confirmation",
        );
        break;
      }
      await sleep(50);
    }
  }
  await sleep(2_000);
  return saveSnapshot(`${command}-${selectedMode}-selected`);
}

async function newChat() {
  await clickElement(
    `[...document.querySelectorAll('button')]
      .filter((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })
      .find((element) => /^(新对话|New chat)$/.test((element.getAttribute('aria-label') || element.innerText || '').trim()))`,
    "New chat button",
  );
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const ready = await evaluate(`Boolean([...document.querySelectorAll('[role="textbox"][contenteditable="true"]')]
      .find((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; }))`);
    if (ready) return;
    await sleep(100);
  }
  throw new Error("New chat composer did not become ready");
}

async function submitPrompt(text) {
  const rect = await clickElement(
    `[...document.querySelectorAll('[role="textbox"][contenteditable="true"]')]
      .find((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })`,
    "composer textbox",
  );
  await send("Input.insertText", { text });
  await sleep(200);
  await saveSnapshot(`${command}-${mode}-prompt-typed`);
  await send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Enter",
    code: "Enter",
    windowsVirtualKeyCode: 13,
    nativeVirtualKeyCode: 36,
  });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Enter",
    code: "Enter",
    windowsVirtualKeyCode: 13,
    nativeVirtualKeyCode: 36,
  });
  await sleep(500);
  return { inputRect: rect };
}

async function waitForTrace(method, direction, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const found = await evaluate(`(() => {
      const events = window[${JSON.stringify(OBSERVER_KEY)}]?.events ?? [];
      return events.some((entry) => entry.direction === ${JSON.stringify(direction)} &&
        (entry.message?.request?.method ?? entry.message?.method ?? null) === ${JSON.stringify(method)});
    })()`);
    if (found) return true;
    await sleep(100);
  }
  return false;
}

await Promise.all([
  send("Page.enable"),
  send("Runtime.enable"),
  send("DOM.enable"),
]);

if (command === "reload-capture") {
  await send("Page.addScriptToEvaluateOnNewDocument", { source: observerExpression });
  await send("Page.reload", { ignoreCache: true });
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const ready = await evaluate(`Boolean(window[${JSON.stringify(OBSERVER_KEY)}]?.export && document.querySelector('[role="textbox"][contenteditable="true"]'))`).catch(() => false);
    if (ready) break;
    await sleep(200);
  }
  await sleep(8_000);
  await saveSnapshot("reload-capture-ready");
  const output = await exportObserver("reload-capture", { observerInstalledBeforeReload: true });
  console.log(JSON.stringify({ outputDir, eventCount: output.trace?.events?.length ?? 0 }, null, 2));
  socket.close();
  process.exit(0);
}

const observer = await installObserver();
if (command === "inspect") {
  const snapshot = await saveSnapshot("inspect-current");
  const output = await exportObserver("inspect-current", { observer, snapshot });
  console.log(JSON.stringify({ outputDir, observer, snapshot, eventCount: output.trace?.events?.length ?? 0 }, null, 2));
} else if (command === "export") {
  const output = await exportObserver("manual-export", { observer });
  console.log(JSON.stringify({ outputDir, eventCount: output.trace?.events?.length ?? 0 }, null, 2));
} else if (command === "select-existing") {
  const before = await saveSnapshot(`${command}-${mode}-before`);
  const after = await selectMode(mode);
  await sleep(1_000);
  const output = await exportObserver(`${command}-${mode}-trace`, { observer, before, after });
  console.log(JSON.stringify({ outputDir, mode, eventCount: output.trace?.events?.length ?? 0 }, null, 2));
} else if (command === "new-thread") {
  await newChat();
  await sleep(1_000);
  const empty = await saveSnapshot(`${command}-${mode}-empty`);
  const selected = await selectMode(mode);
  const submission = await submitPrompt(prompt);
  const sawThreadStart = await waitForTrace("thread/start", "renderer_to_host", 30_000);
  const sawTurnCompleted = await waitForTrace("turn/completed", "host_to_renderer", 120_000);
  await sleep(1_000);
  const final = await saveSnapshot(`${command}-${mode}-final`);
  const output = await exportObserver(`${command}-${mode}-trace`, {
    observer,
    empty,
    selected,
    submission,
    sawThreadStart,
    sawTurnCompleted,
    final,
  });
  console.log(JSON.stringify({
    outputDir,
    mode,
    sawThreadStart,
    sawTurnCompleted,
    eventCount: output.trace?.events?.length ?? 0,
  }, null, 2));
}

socket.close();
