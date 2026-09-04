#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const option = (name, fallback = null) => {
  const prefix = `--${name}=`;
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) ?? fallback;
};

const command = process.argv[2] ?? "inspect";
const endpoint = option("endpoint", "http://127.0.0.1:9323");
const outputDir = path.resolve(option("artifact-dir", "/tmp/gpui-mcp-tool-ui-evidence"));
const name = option("name", command);
const text = option("text", "");
const OBSERVER_KEY = "__gpuiMcpToolCallEvidenceV1";

fs.mkdirSync(outputDir, { recursive: true });

const targets = await (await fetch(`${endpoint}/json/list`)).json();
const target = targets.find(
  (candidate) => candidate.type === "page" && candidate.url === "app://-/index.html",
);
if (!target) throw new Error(`ChatGPT app://-/index.html target not found on ${endpoint}`);

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
  const response = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (response.exceptionDetails) {
    throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
  }
  return response.result.value;
}

const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

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

async function screenshot(snapshotName) {
  const image = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(path.join(outputDir, `${snapshotName}.png`), Buffer.from(image.data, "base64"));
}

const observerExpression = `(() => {
  const observerKey = ${JSON.stringify(OBSERVER_KEY)};
  window[observerKey]?.dispose?.();
  const state = {
    version: 1,
    installedAt: new Date().toISOString(),
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
    if (typeof value === 'string') return value.length > 100000 ? value.slice(0, 100000) : value;
    if (typeof value === 'bigint') return String(value);
    if (typeof value === 'function') return '[Function]';
    if (typeof value !== 'object') return String(value);
    if (depth >= 16) return '[MaxDepth]';
    if (seen.has(value)) return '[Circular]';
    seen.add(value);
    if (Array.isArray(value)) return value.slice(0, 10000).map((item) => safe(item, depth + 1, seen));
    const output = {};
    for (const property of Object.keys(value).slice(0, 10000)) {
      try { output[property] = safe(value[property], depth + 1, seen, property); }
      catch (error) { output[property] = '[Unreadable: ' + String(error) + ']'; }
    }
    return output;
  };
  const methodOf = (message) => message?.request?.method ?? message?.method ?? null;
  const paramsOf = (message) => message?.request?.params ?? message?.params ?? null;
  const isMcpItem = (message) => paramsOf(message)?.item?.type === 'mcpToolCall';
  const watchedMethods = new Set([
    'thread/start', 'thread/resume', 'turn/start', 'turn/started', 'turn/completed',
    'item/started', 'item/completed', 'tool/requestUserInput',
    'item/tool/requestUserInput', 'serverRequest/resolved',
  ]);
  const relevant = (message) => {
    if (message == null || typeof message !== 'object') return false;
    if (message.type === 'mcp-request') {
      const method = methodOf(message) || '';
      const matches = watchedMethods.has(method);
      if (matches && message.request?.id != null) state.watchedRequestIds.add(message.request.id);
      return matches;
    }
    if (message.type === 'mcp-notification') {
      const method = methodOf(message) || '';
      return method === 'item/started' || method === 'item/completed' ? isMcpItem(message) : watchedMethods.has(method);
    }
    if (message.type === 'mcp-response') {
      const id = message.message?.id;
      if (id == null || !state.watchedRequestIds.has(id)) return false;
      state.watchedRequestIds.delete(id);
      return true;
    }
    const method = methodOf(message) || '';
    return (method === 'item/started' || method === 'item/completed') ? isMcpItem(message) : watchedMethods.has(method);
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
        case 'array-start': { const value = []; saveValue(assembler, value); assembler.stack.push({ type: 'array', value }); break; }
        case 'object-start': { const value = {}; saveValue(assembler, value); assembler.stack.push({ type: 'object', value, key: null }); break; }
        case 'container-end': assembler.stack.pop(); break;
        case 'key': assembler.stack.at(-1).key = token.value; break;
        case 'value': saveValue(assembler, token.value); break;
        case 'string-start': assembler.stringChunks = []; assembler.stringTarget = token.target; break;
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
    if (data.kind === 'chunk') { consume(transfer.assembler, data.tokens || []); return null; }
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
  state.export = () => ({
    version: state.version,
    installedAt: state.installedAt,
    exportedAt: new Date().toISOString(),
    events: safe(state.events),
    rawChunkEvents: safe(state.rawChunkEvents),
    activeTransfers: state.transfers.size,
  });
  window[observerKey] = state;
  return {
    installedAt: state.installedAt,
    appSessionId: window.electronBridge?.getAppSessionId?.() ?? null,
    buildFlavor: window.electronBridge?.getBuildFlavor?.() ?? null,
  };
})()`;

async function domSnapshot() {
  return evaluate(`(() => {
    const rect = (element) => {
      const value = element.getBoundingClientRect();
      return { x: value.x, y: value.y, width: value.width, height: value.height, top: value.top, right: value.right, bottom: value.bottom, left: value.left };
    };
    const style = (element) => {
      const value = getComputedStyle(element);
      const keys = [
        'display','position','boxSizing','flexDirection','alignItems','justifyContent','gap','width','height',
        'minWidth','minHeight','maxWidth','maxHeight','paddingTop','paddingRight','paddingBottom','paddingLeft',
        'marginTop','marginRight','marginBottom','marginLeft','backgroundColor','color','borderTopWidth',
        'borderRightWidth','borderBottomWidth','borderLeftWidth','borderTopColor','borderRightColor',
        'borderBottomColor','borderLeftColor','borderRadius','boxShadow','fontFamily','fontSize','fontWeight',
        'lineHeight','letterSpacing','opacity','cursor','overflowX','overflowY','whiteSpace','textOverflow',
        'transition','transform'
      ];
      return Object.fromEntries(keys.map((key) => [key, value[key]]));
    };
    const describe = (element) => ({
      tag: element.tagName,
      id: element.id || null,
      className: typeof element.className === 'string' ? element.className : null,
      text: (element.innerText || element.textContent || '').trim().slice(0, 12000),
      attributes: Object.fromEntries([...element.attributes]
        .filter((attribute) => attribute.name.startsWith('data-') || attribute.name.startsWith('aria-') || ['role','title'].includes(attribute.name))
        .map((attribute) => [attribute.name, attribute.value])),
      rect: rect(element),
      style: style(element),
      outerHTML: element.outerHTML.slice(0, 50000),
    });
    const input = document.querySelector('[role="textbox"][contenteditable="true"]');
    const inputTop = input?.getBoundingClientRect().top ?? innerHeight;
    const matcher = /(?:mcp|tool|usage|limit|codex app|get_usage|create_thread|调用|工具|额度|使用量|批准|审批|允许|拒绝|取消|accept|decline|cancel|argument|result|error)/i;
    const candidates = [...document.querySelectorAll('*')].filter((element) => {
      const bounds = element.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0 || bounds.top < 30 || bounds.top >= inputTop + 1) return false;
      const metadata = [element.innerText, element.textContent, element.getAttribute('aria-label'), element.getAttribute('title'), element.getAttribute('data-testid')].filter(Boolean).join(' ');
      return matcher.test(metadata);
    }).map(describe).sort((left, right) => (left.rect.width * left.rect.height) - (right.rect.width * right.rect.height)).slice(0, 160);
    const buttons = [...document.querySelectorAll('button,[role="button"]')]
      .filter((element) => { const bounds = element.getBoundingClientRect(); return bounds.width > 0 && bounds.height > 0; })
      .map(describe).slice(-160);
    return {
      capturedAt: new Date().toISOString(),
      href: location.href,
      title: document.title,
      viewport: {
        innerWidth, innerHeight, outerWidth, outerHeight, devicePixelRatio,
        screen: { width: screen.width, height: screen.height, availWidth: screen.availWidth, availHeight: screen.availHeight },
      },
      scroll: { x: scrollX, y: scrollY },
      bodyTextTail: document.body.innerText.slice(-20000),
      candidates,
      buttons,
    };
  })()`);
}

async function saveSnapshot(snapshotName) {
  const snapshot = await domSnapshot();
  fs.writeFileSync(path.join(outputDir, `${snapshotName}.dom.json`), JSON.stringify(snapshot, null, 2));
  await screenshot(snapshotName);
  return snapshot;
}

async function exportTrace(traceName) {
  const trace = await evaluate(`window[${JSON.stringify(OBSERVER_KEY)}]?.export?.() ?? null`);
  const payload = {
    captureVersion: 1,
    capturedAt: new Date().toISOString(),
    endpoint,
    target: { id: target.id, title: target.title, type: target.type, url: target.url },
    trace,
  };
  fs.writeFileSync(path.join(outputDir, `${traceName}.trace.json`), JSON.stringify(payload, null, 2));
  return payload;
}

async function waitFor(kind, timeoutMs = 55_000) {
  const predicates = {
    started: `(method === 'item/started' && params?.item?.type === 'mcpToolCall')`,
    completed: `(method === 'item/completed' && params?.item?.type === 'mcpToolCall')`,
    approval: `(method === 'tool/requestUserInput' || method === 'item/tool/requestUserInput')`,
    resume: `(method === 'thread/resume')`,
    turn: `(method === 'turn/completed')`,
  };
  const predicate = predicates[kind];
  if (!predicate) throw new Error(`Unsupported wait kind: ${kind}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const matched = await evaluate(`(() => (window[${JSON.stringify(OBSERVER_KEY)}]?.events ?? []).some((entry) => {
      const message = entry.message;
      const method = message?.request?.method ?? message?.method ?? null;
      const params = message?.request?.params ?? message?.params ?? null;
      return ${predicate};
    }))()`);
    if (matched) return true;
    await sleep(100);
  }
  return false;
}

await Promise.all([send("Page.enable"), send("Runtime.enable"), send("DOM.enable")]);

let result;
switch (command) {
  case "install":
    result = await evaluate(observerExpression);
    break;
  case "new-chat":
    result = await clickElement(
      `[...document.querySelectorAll('button,[role="button"]')]
        .filter((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })
        .find((element) => /^(?:新对话|New chat)$/.test((element.getAttribute('aria-label') || element.innerText || '').trim()))`,
      "New chat button",
    );
    break;
  case "submit":
    if (!text) throw new Error("submit requires --text=...");
    result = await clickElement(
      `[...document.querySelectorAll('[role="textbox"][contenteditable="true"]')]
        .find((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })`,
      "composer textbox",
    );
    await send("Input.insertText", { text });
    await sleep(150);
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 36 });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 36 });
    break;
  case "wait":
    result = { kind: text, matched: await waitFor(text) };
    break;
  case "snapshot":
    result = await saveSnapshot(name);
    break;
  case "export":
    result = await exportTrace(name);
    break;
  case "click-text":
    if (!text) throw new Error("click-text requires --text=...");
    result = await clickElement(
      `[...document.querySelectorAll('button,[role="button"]')]
        .filter((element) => { const rect = element.getBoundingClientRect(); return rect.width > 0 && rect.height > 0; })
        .find((element) => (element.innerText || element.textContent || element.getAttribute('aria-label') || '').trim().includes(${JSON.stringify(text)}))`,
      `button containing ${text}`,
    );
    break;
  case "reload":
    await send("Page.addScriptToEvaluateOnNewDocument", { source: observerExpression });
    result = await send("Page.reload", { ignoreCache: true });
    break;
  case "inspect":
    result = {
      target: { id: target.id, title: target.title, type: target.type, url: target.url },
      observer: await evaluate(`window[${JSON.stringify(OBSERVER_KEY)}]?.export?.() ?? null`),
      dom: await domSnapshot(),
    };
    break;
  default:
    throw new Error(`Unsupported command: ${command}`);
}

process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
socket.close();
