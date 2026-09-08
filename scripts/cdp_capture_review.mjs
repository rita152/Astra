// Capture only the visible review pane in a separately launched ChatGPT instance.
// Require an explicit endpoint; never attach to a default port used by another task.
import fs from 'node:fs';
const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = process.argv[2] || 'artifacts/review-reference';
fs.mkdirSync(output, { recursive: true });
const page = (await (await fetch(`${endpoint}/json/list`)).json()).find(t => t.url === 'app://-/index.html');
if (!page) throw Error('ChatGPT main window not found');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let id = 0;
const pending = new Map();
socket.onmessage = event => {
  const m = JSON.parse(event.data), p = pending.get(m.id);
  if (!p) return;
  pending.delete(m.id); clearTimeout(p.timer);
  m.error ? p.reject(Error(m.error.message)) : p.resolve(m.result);
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const request = ++id;
  const timer = setTimeout(() => { pending.delete(request); reject(Error(`${method} timed out`)); }, 10000);
  pending.set(request, { resolve, reject, timer });
  socket.send(JSON.stringify({ id: request, method, params }));
});
try {
  const r = await send('Runtime.evaluate', { returnByValue: true, expression: `(() => {
    const roots = [document];
    const walk = root => { for (const e of root.querySelectorAll('*')) if (e.shadowRoot) { roots.push(e.shadowRoot); walk(e.shadowRoot); } };
    walk(document);
    const properties = ['fontFamily','fontSize','fontWeight','lineHeight','color','backgroundColor','border','borderRadius','padding','gap','overflow','width','height'];
    const visible = e => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.bottom > 0 && r.y < innerHeight; };
    const nodes = roots.flatMap(root => [...root.querySelectorAll('button,input,[role=menu],[role=menuitem],[role=dialog],[contenteditable],[data-line],[data-column-number],[data-diff],[data-item-path]')]).filter(visible);
    return { viewport:[innerWidth,innerHeight], dpr:devicePixelRatio, theme:document.documentElement.className,
      nodes:nodes.map(e => ({ tag:e.tagName, role:e.getAttribute('role'), label:e.getAttribute('aria-label'),
        text:e.textContent.slice(0,250), placeholder:e.getAttribute('placeholder'), disabled:e.getAttribute('aria-disabled') || e.disabled,
        rect:e.getBoundingClientRect().toJSON(), style:Object.fromEntries(properties.map(p => [p,getComputedStyle(e)[p]])) })) };
  })()` });
  if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
  const data = r.result.value;
  const name = `review-${data.theme.includes('dark') ? 'dark' : 'light'}`;
  fs.writeFileSync(`${output}/${name}.json`, JSON.stringify(data, null, 2));
  const shot = await send('Page.captureScreenshot', { format:'png', captureBeyondViewport:false });
  fs.writeFileSync(`${output}/${name}.png`, Buffer.from(shot.data, 'base64'));
  console.log(JSON.stringify({ output, name, viewport:data.viewport, nodes:data.nodes.length }));
} finally {
  for (const p of pending.values()) clearTimeout(p.timer);
  socket.close();
}
