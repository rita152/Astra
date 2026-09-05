// Controlled live-text fixture inside the real ChatGPT renderer. This is a
// rasterizer comparison, not a claim that the fixture is a product screen.
import fs from 'node:fs';
import path from 'node:path';
const endpoint = process.env.CHATGPT_CDP_HTTP || 'http://127.0.0.1:9222';
const output = path.resolve(process.argv[2] || 'artifacts/typography-specimen');
const dpr = Number(process.argv[3] || 1);
const samples = JSON.parse(fs.readFileSync(new URL('./typography_samples.json', import.meta.url)));
const targets = await (await fetch(`${endpoint}/json/list`)).json();
const target = targets.find(t => t.type === 'page' && t.title === 'ChatGPT' && t.url === 'app://-/index.html');
if (!target) throw new Error(`ChatGPT target missing at ${endpoint}`);
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let id = 0;
const pending = new Map();
socket.onmessage = ({data}) => {
  const message = JSON.parse(data), callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  message.error ? callback.reject(new Error(JSON.stringify(message.error))) : callback.resolve(message.result);
};
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    pending.set(++id, {resolve, reject});
    socket.send(JSON.stringify({id, method, params}));
  });
}
async function evaluate(expression) {
  const result = await send('Runtime.evaluate', {expression, returnByValue:true, awaitPromise:true});
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
try {
  await send('Emulation.setDeviceMetricsOverride', {width:1000,height:620,deviceScaleFactor:dpr,mobile:false});
  const metadata = await evaluate(`(async () => {
    const samples = ${JSON.stringify(samples)};
    const bodyStyle = getComputedStyle(document.body);
    const root = document.createElement('div');
    root.id = 'gpui-typography-specimen';
    root.style.cssText = 'position:fixed;inset:0;width:1000px;height:620px;z-index:2147483647;pointer-events:none;';
    document.body.append(root);
    const rows = [];
    for (const dark of [false,true]) {
      const panel = document.createElement('div');
      panel.style.cssText = 'position:absolute;top:0;width:500px;height:620px;';
      panel.style.left = dark ? '500px' : '0px';
      panel.style.background = dark ? '#181818' : '#ffffff';
      panel.style.color = dark ? '#dfdfdf' : '#1a1c1f';
      root.append(panel);
      samples.forEach((sample,index) => {
        const row = document.createElement('div');
        row.style.cssText = 'position:absolute;left:24px;white-space:pre;';
        Object.assign(row.style, {top:(40+index*36)+'px',fontFamily:sample.brand?'"OpenAI Sans"':sample.mono?'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace':bodyStyle.fontFamily,fontSize:sample.size+'px',fontWeight:String(sample.weight),lineHeight:sample.height+'px',webkitFontSmoothing:bodyStyle.webkitFontSmoothing});
        row.textContent = sample.text;
        panel.append(row);
        rows.push({dark,index,row});
      });
    }
    await document.fonts.ready;
    await new Promise(requestAnimationFrame);
    return {fixture:true,dpr:devicePixelRatio,smoothing:bodyStyle.webkitFontSmoothing,bodyWeight:bodyStyle.fontWeight,rows:rows.map(({dark,index,row})=>{
      const range=document.createRange();range.selectNodeContents(row);const r=range.getBoundingClientRect();
      return {dark,index,text:row.textContent,rect:{x:r.x,y:r.y,width:r.width,height:r.height},style:row.style.cssText};
    })};
  })()`);
  await send('DOM.enable'); await send('CSS.enable');
  const {root} = await send('DOM.getDocument');
  const {nodeIds} = await send('DOM.querySelectorAll',{nodeId:root.nodeId,selector:'#gpui-typography-specimen > div > div'});
  for (let index=0;index<nodeIds.length;index++) {
    metadata.rows[index].platformFonts=(await send('CSS.getPlatformFontsForNode',{nodeId:nodeIds[index]})).fonts;
  }
  const screenshot=await send('Page.captureScreenshot',{format:'png',fromSurface:true,clip:{x:0,y:0,width:1000,height:620,scale:1}});
  fs.mkdirSync(output,{recursive:true});
  fs.writeFileSync(path.join(output,`electron-${dpr}x.png`),Buffer.from(screenshot.data,'base64'));
  fs.writeFileSync(path.join(output,`electron-${dpr}x.json`),JSON.stringify(metadata,null,2)+'\n');
  console.log(`Saved ${output}/electron-${dpr}x.png`);
} finally {
  await evaluate(`document.getElementById('gpui-typography-specimen')?.remove()`);
  await send('Emulation.clearDeviceMetricsOverride');
  socket.close();
}
