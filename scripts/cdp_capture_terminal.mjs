// Inspect the terminal in a dedicated ChatGPT instance, already opened in the side panel.
// Runs only a harmless printf command in its terminal; never sends a chat prompt.
import fs from 'node:fs';
const endpoint = process.env.CHATGPT_CDP_HTTP || 'http://127.0.0.1:9222';
const output = process.argv[2] || 'artifacts/terminal';
fs.mkdirSync(output, {recursive:true});
const targets = await (await fetch(`${endpoint}/json/list`)).json();
const target = targets.find(t => t.url === 'app://-/index.html');
if (!target) throw Error('ChatGPT page not found');
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
let id=0;
const pending=new Map();
socket.onmessage=event=>{const m=JSON.parse(event.data);const p=pending.get(m.id);if(p){pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result);}};
const send=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});socket.send(JSON.stringify({id:n,method,params}));});
const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
const styles=await evaluate(`(() => {
 const props=['fontFamily','fontSize','fontWeight','lineHeight','color','backgroundColor','padding','borderRadius','height','width'];
 const terminal=document.querySelector('.xterm'); if(!terminal)throw Error('Open a terminal in the dedicated debug instance first');
 return {viewport:[innerWidth,innerHeight],dpr:devicePixelRatio,theme:document.documentElement.className,
 nodes:[terminal,terminal.parentElement.parentElement,...terminal.querySelectorAll('.xterm-rows,.xterm-rows>div:first-child,.xterm-cursor')].map(e=>({class:e.className,rect:e.getBoundingClientRect().toJSON(),style:Object.fromEntries(props.map(p=>[p,getComputedStyle(e)[p]]))}))};
})()`);
fs.writeFileSync(`${output}/cdp-styles.json`,JSON.stringify(styles,null,2));
await evaluate(`document.querySelector('.xterm-helper-textarea').focus()`);
await send('Input.dispatchKeyEvent',{type:'keyDown',key:'u',code:'KeyU',windowsVirtualKeyCode:85,modifiers:2});
await send('Input.dispatchKeyEvent',{type:'keyUp',key:'u',code:'KeyU',windowsVirtualKeyCode:85,modifiers:2});
const marker = `GPUI_TERMINAL_SAMPLE_${Date.now()}`;
await send('Input.dispatchKeyEvent',{type:'keyDown',key:'l',code:'KeyL',windowsVirtualKeyCode:76,modifiers:2});
await send('Input.dispatchKeyEvent',{type:'keyUp',key:'l',code:'KeyL',windowsVirtualKeyCode:76,modifiers:2});
await send('Input.insertText',{text:`printf '\\033[31mGPUI_TERMINAL_RED\\033[0m 中文 ✓\\n'; stty size; printf '${marker}\\n'`});
for(const type of ['keyDown','keyUp'])await send('Input.dispatchKeyEvent',{type,key:'Enter',code:'Enter',windowsVirtualKeyCode:13,text:type==='keyDown'?'\r':undefined});
let rows='';
for(let i=0;i<30;i++){rows=await evaluate(`document.querySelector('.xterm-rows').innerText`);if(rows.includes(`\n${marker}\n`))break;await new Promise(r=>setTimeout(r,100));}
if (!rows.includes(`\n${marker}\n`)) throw Error('Terminal sample did not finish');
fs.writeFileSync(`${output}/reference-output.txt`,rows);
const shot=await send('Page.captureScreenshot',{format:'png'});
fs.writeFileSync(`${output}/reference-output.png`,Buffer.from(shot.data,'base64'));
socket.close();
console.log(JSON.stringify({output,styles,rows},null,2));
