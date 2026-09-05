import fs from "node:fs";
import path from "node:path";

const CDP_HTTP = process.env.CHATGPT_CDP_HTTP || "http://127.0.0.1:9222";
const outputDir = path.resolve(
  process.argv.find((argument) => argument.startsWith("--artifact-dir="))
    ?.slice("--artifact-dir=".length) ||
    "artifacts/chatgpt-typography-2026-08-29",
);
fs.mkdirSync(outputDir, { recursive: true });

const targets = await (await fetch(`${CDP_HTTP}/json/list`)).json();
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.title === "ChatGPT" &&
    candidate.url === "app://-/index.html",
);
if (!target) throw new Error(`ChatGPT app target not found at ${CDP_HTTP}`);

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 0;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
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

async function evaluate(expression, returnByValue = true) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result;
}

const smallestVisibleTextElement = (text) => `(() => {
  const matches = [...document.querySelectorAll('button, [role], div, span, p')]
    .filter((element) => {
      const rect = element.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0 &&
        (element.innerText || element.textContent || '').trim().includes(${JSON.stringify(text)});
    })
    .sort((a, b) => {
      const aRect = a.getBoundingClientRect();
      const bRect = b.getBoundingClientRect();
      return aRect.width * aRect.height - bRect.width * bRect.height;
    });
  return matches[0] || null;
})()`;

const smallestVisibleDescendant = (selector) => `(() => {
  const root = document.querySelector(${JSON.stringify(selector)});
  if (!root) return null;
  const matches = [root, ...root.querySelectorAll('*')]
    .filter((element) => {
      const rect = element.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0 &&
        (element.innerText || element.textContent || '').trim().length > 0;
    })
    .sort((a, b) => {
      const aRect = a.getBoundingClientRect();
      const bRect = b.getBoundingClientRect();
      return aRect.width * aRect.height - bRect.width * bRect.height;
    });
  return matches[0] || null;
})()`;

const candidates = [
  ["body", "document.body"],
  ["composer", "document.querySelector('[role=\"textbox\"][contenteditable=\"true\"]')"],
  ["assistant-message", smallestVisibleDescendant('[data-markdown-text-style="assistant-message"]')],
  ["user-message", smallestVisibleDescendant('[data-user-message-bubble="true"]')],
  ["product-title", smallestVisibleTextElement("Codex")],
  ["sidebar-new-chat", smallestVisibleTextElement("新对话")],
  ["sidebar-english-thread", smallestVisibleTextElement("Respond to greeting")],
  ["mixed-command-row", smallestVisibleTextElement("已运行 printf")],
  ["shell-output", smallestVisibleTextElement("COMMAND_UI_REFERENCE")],
];

await Promise.all([
  send("DOM.enable"),
  send("CSS.enable"),
  send("Runtime.enable"),
  send("Page.enable"),
]);
// Populate the frontend DOM tree so Runtime object handles can be mapped to
// stable node ids accepted by the CSS domain.
await send("DOM.getDocument", { depth: -1, pierce: true });

const nodes = [];
for (const [name, expression] of candidates) {
  const remote = await evaluate(expression, false);
  if (!remote.objectId || remote.subtype === "null") {
    nodes.push({ name, found: false });
    continue;
  }
  const { nodeId } = await send("DOM.requestNode", { objectId: remote.objectId });
  const [{ computedStyle }, { fonts }, metadata] = await Promise.all([
    send("CSS.getComputedStyleForNode", { nodeId }),
    send("CSS.getPlatformFontsForNode", { nodeId }),
    send("Runtime.callFunctionOn", {
      objectId: remote.objectId,
      functionDeclaration: `function () {
        const rect = this.getBoundingClientRect();
        return {
          tag: this.tagName,
          text: (this.innerText || this.textContent || '').trim().slice(0, 1000),
          className: typeof this.className === 'string' ? this.className : null,
          rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        };
      }`,
      returnByValue: true,
    }),
  ]);
  const interestingProperties = new Set([
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-stretch",
    "font-feature-settings",
    "font-variation-settings",
    "line-height",
    "letter-spacing",
    "-webkit-font-smoothing",
    "text-rendering",
  ]);
  nodes.push({
    name,
    found: true,
    ...metadata.result.value,
    computedStyle: Object.fromEntries(
      computedStyle
        .filter(({ name: property }) => interestingProperties.has(property))
        .map(({ name: property, value }) => [property, value]),
    ),
    platformFonts: fonts,
  });
  await send("Runtime.releaseObject", { objectId: remote.objectId });
}

const documentFonts = (
  await evaluate(`[...document.fonts].map((font) => ({
    family: font.family,
    style: font.style,
    weight: font.weight,
    stretch: font.stretch,
    status: font.status,
  }))`)
).value;

const payload = {
  capturedAt: new Date().toISOString(),
  target: { id: target.id, title: target.title, url: target.url },
  navigator: (
    await evaluate(`({
      platform: navigator.platform,
      userAgent: navigator.userAgent,
      language: navigator.language,
      languages: navigator.languages,
      devicePixelRatio,
    })`)
  ).value,
  nodes,
  documentFonts,
};

const outputPath = path.join(outputDir, "typography.json");
fs.writeFileSync(outputPath, `${JSON.stringify(payload, null, 2)}\n`);
const screenshot = await send("Page.captureScreenshot", {
  format: "png",
  fromSurface: true,
  captureBeyondViewport: false,
});
fs.writeFileSync(
  path.join(outputDir, "chatgpt-reference.png"),
  Buffer.from(screenshot.data, "base64"),
);
console.log(outputPath);
socket.close();
