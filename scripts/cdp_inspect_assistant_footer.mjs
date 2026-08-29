import fs from "node:fs";
import path from "node:path";

const screenshotPath = process.argv
  .find((argument) => argument.startsWith("--screenshot="))
  ?.slice("--screenshot=".length);

const targets = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const target = targets.find((candidate) =>
  candidate.type === "page" && candidate.title === "ChatGPT" && candidate.url === "app://-/index.html"
);
if (!target) throw new Error("ChatGPT app target not found");

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", ({ data }) => {
  const message = JSON.parse(data);
  const callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  message.error ? callback.reject(message.error) : callback.resolve(message.result);
});
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = nextId++;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result.value;
};

await send("Runtime.enable");
const candidates = await evaluate(`(() => [...document.querySelectorAll('button')]
  .filter((button) => /复制|赞|踩|分支|重试/.test(button.getAttribute('aria-label') || ''))
  .map((button) => {
    const rect = button.getBoundingClientRect();
    return {label: button.getAttribute('aria-label'), x: rect.x, y: rect.y, width: rect.width, height: rect.height};
  }))()`);
console.log(JSON.stringify({ candidates }, null, 2));

const targetButton = await evaluate(`(() => {
  const buttons = [...document.querySelectorAll('button')]
    .filter((button) => /复制/.test(button.getAttribute('aria-label') || ''));
  const button = buttons.at(-1);
  button?.scrollIntoView({block:'center'});
  return Boolean(button);
})()`);
if (!targetButton) throw new Error("assistant copy action not found");
await new Promise((resolve) => setTimeout(resolve, 200));

const ancestry = await evaluate(`(() => {
  const copy = [...document.querySelectorAll('button')]
    .filter((button) => (button.getAttribute('aria-label') || '') === '复制').at(-1);
  const values = [];
  for (let node = copy; node && values.length < 10; node = node.parentElement) {
    const rect = node.getBoundingClientRect();
    values.push({tag:node.tagName,className:node.className,
      rect:{x:rect.x,y:rect.y,width:rect.width,height:rect.height},
      text:(node.innerText || '').trim().slice(0,200)});
  }
  const time = [...document.querySelectorAll('span')]
    .filter((span) => /^\\d{2}:\\d{2}$/.test((span.textContent || '').trim())).at(-1);
  return {values,timeHTML:time?.outerHTML,timeParentHTML:time?.parentElement?.outerHTML.slice(0,2000)};
})()`);
console.log(JSON.stringify({ ancestry }, null, 2));

await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 20, y: 400 });
await new Promise((resolve) => setTimeout(resolve, 100));
const before = await evaluate(inspectExpression());
const hoverPoint = before?.contentRect
  ? { x: before.contentRect.x + 20, y: before.contentRect.y + Math.min(10, before.contentRect.height / 2) }
  : { x: before.actionsRect.x + 10, y: before.actionsRect.y + 13 };
await send("Input.dispatchMouseEvent", { type: "mouseMoved", ...hoverPoint });
await new Promise((resolve) => setTimeout(resolve, 100));
const contentHover = await evaluate(inspectExpression());
await send("Input.dispatchMouseEvent", {
  type: "mouseMoved",
  x: contentHover.actionsRect.x + 10,
  y: contentHover.actionsRect.y + 13,
});
await new Promise((resolve) => setTimeout(resolve, 100));
const actionsHover = await evaluate(inspectExpression());

console.log(JSON.stringify({ before, contentHover, actionsHover }, null, 2));
if (screenshotPath) {
  await send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: actionsHover.groupRect.x + 200,
    y: actionsHover.groupRect.y - 20,
  });
  await new Promise((resolve) => setTimeout(resolve, 100));
  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.mkdirSync(path.dirname(screenshotPath), { recursive: true });
  fs.writeFileSync(screenshotPath, Buffer.from(screenshot.data, "base64"));
  console.log(JSON.stringify({ screenshot: screenshotPath }));
}
socket.close();

function inspectExpression() {
  return `(() => {
    const copyButtons = [...document.querySelectorAll('button')]
      .filter((button) => /复制/.test(button.getAttribute('aria-label') || ''));
    const copy = copyButtons.at(-1);
    if (!copy) return null;
    let actions = copy.parentElement;
    while (actions && actions.querySelectorAll('button').length < 4) actions = actions.parentElement;
    let group = actions;
    while (group && ![...group.querySelectorAll('span')]
      .some((span) => /^\\d{2}:\\d{2}$/.test((span.textContent || '').trim()))) group = group.parentElement;
    const time = [...(group?.querySelectorAll('span') || [])]
      .find((span) => /^\\d{2}:\\d{2}$/.test((span.textContent || '').trim()));
    const content = group?.firstElementChild;
    const rect = (element) => {
      const value = element?.getBoundingClientRect();
      return value ? {x:value.x,y:value.y,width:value.width,height:value.height,right:value.right,bottom:value.bottom} : null;
    };
    const style = (element) => {
      if (!element) return null;
      const value = getComputedStyle(element);
      return {
        display:value.display, visibility:value.visibility, opacity:value.opacity,
        color:value.color, fontSize:value.fontSize, lineHeight:value.lineHeight,
        margin:value.margin, padding:value.padding, gap:value.gap,
        justifyContent:value.justifyContent, alignItems:value.alignItems,
      };
    };
    return {
      groupTag:group?.tagName, groupClass:group?.className,
      groupRect:rect(group), contentRect:rect(content), actionsRect:rect(actions),
      actionsStyle:style(actions),
      time:{text:(time?.textContent || '').trim(), rect:rect(time), style:style(time)},
      buttons:[...(actions?.querySelectorAll('button') || [])].map((button) => ({
        label:button.getAttribute('aria-label'), rect:rect(button), style:style(button),
        svgRect:rect(button.querySelector('svg')),
      })),
      groupHTML:group?.outerHTML.slice(0, 5000),
    };
  })()`;
}
