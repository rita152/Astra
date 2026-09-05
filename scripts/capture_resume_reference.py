#!/usr/bin/env python3
"""Capture real resumed ChatGPT threads and computed styles through local CDP.

Run against a dedicated ChatGPT debug instance. Only navigates existing threads,
changes appearance, and expands disclosure controls; never submits a prompt.
"""
import argparse
import base64
import json
from pathlib import Path
import time
import urllib.request

from cdp_chatgpt import Cdp

STYLE_PROBE = r"""(() => {
 const root = document.querySelector('.thread-scroll-container');
 const properties = ['font-family','font-size','font-weight','line-height','color',
 'background-color','border-color','border-width','border-radius','padding',
 'margin','gap','max-width','width','height','display','overflow'];
 const nodes = [...root.querySelectorAll('[data-markdown-text-style],p,h1,h2,h3,h4,li,ul,ol,pre,code,table,th,td,blockquote,hr,button,[data-user-message-bubble], [class*="activity-header"], [data-markdown-copy]')];
 return {size:[innerWidth,innerHeight],dpr:devicePixelRatio,
 theme:document.documentElement.className,scrollTop:root.scrollTop,
 scrollHeight:root.scrollHeight,clientHeight:root.clientHeight,
 nodes:nodes.map(e=>({tag:e.tagName,text:(e.innerText||e.textContent||"").slice(0,160),class:e.className,
 rect:e.getBoundingClientRect().toJSON(),
 style:Object.fromEntries(properties.map(p=>[p,getComputedStyle(e).getPropertyValue(p)]))}))};
})()"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', default='http://127.0.0.1:9222')
    parser.add_argument('--output', type=Path, default=Path('artifacts/resume-alignment'))
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--include-upper', action='store_true', help='Also capture 600px above the bottom')
    args = parser.parse_args()
    targets = json.load(urllib.request.urlopen(args.endpoint + '/json/list'))
    target = next(t for t in targets if t['url'] == 'app://-/index.html')
    cdp = Cdp(target['webSocketDebuggerUrl'])

    def evaluate(expression):
        result = cdp.call('Runtime.evaluate', {'expression': expression, 'returnByValue': True, 'awaitPromise': True})
        if result.get('exceptionDetails'):
            raise RuntimeError(result['exceptionDetails'])
        return result.get('result', {}).get('value')

    def wait_for(expression):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if evaluate(expression):
                return
            time.sleep(.2)
        raise RuntimeError('UI did not settle: ' + expression)

    def click_text(text):
        evaluate(f"[...document.querySelectorAll('button')].find(e=>e.innerText==={json.dumps(text)}).click()")

    def click_selector(selector):
        rect = evaluate(f"document.querySelector({json.dumps(selector)}).getBoundingClientRect().toJSON()")
        point = {'x': rect['x'] + rect['width']/2, 'y': rect['y'] + rect['height']/2}
        for kind in ['mousePressed', 'mouseReleased']:
            cdp.call('Input.dispatchMouseEvent', {'type':kind, **point, 'button':'left','clickCount':1})

    def align_sidebar():
        rect = evaluate("document.querySelector('[role=separator][aria-orientation=vertical]')?.getBoundingClientRect().toJSON()")
        if not rect:
            raise RuntimeError('Expand the sidebar before capturing resumed threads')
        x, y = rect['x'] + rect['width']/2, 400
        if abs(x - 240) <= .5:
            return
        cdp.call('Input.dispatchMouseEvent', {'type':'mousePressed','x':x,'y':y,'button':'left','buttons':1,'clickCount':1})
        for step in range(1, 9):
            cdp.call('Input.dispatchMouseEvent', {'type':'mouseMoved','x':x+(240-x)*step/8,'y':y,'button':'left','buttons':1})
        cdp.call('Input.dispatchMouseEvent', {'type':'mouseReleased','x':240,'y':y,'button':'left','buttons':0,'clickCount':1})

    try:
        cdp.call('Emulation.setDeviceMetricsOverride', {'width':1440,'height':900,'deviceScaleFactor':1,'mobile':False})
        samples = json.loads(args.manifest.read_text())
        for theme, label in [('light','浅色'),('dark','深色')]:
            if not evaluate("!!document.querySelector('input[name=appearance-theme]')"):
                if not evaluate("!!document.querySelector('[role=menuitem]')"):
                    click_selector('[aria-label="打开个人资料菜单"]')
                wait_for("!!document.querySelector('[role=menuitem]')")
                evaluate("[...document.querySelectorAll('[role=menuitem]')].find(e=>e.innerText.includes('设置')).click()")
                wait_for("[...document.querySelectorAll('button')].some(e=>e.innerText==='外观')")
                click_text('外观')
            wait_for("!!document.querySelector('input[name=appearance-theme]')")
            evaluate(f"document.querySelector('input[name=appearance-theme][aria-label=\"{label}\"]').click()")
            wait_for(f"document.documentElement.classList.contains('electron-{theme}')")
            click_text('返回应用')
            wait_for("!!document.querySelector('[data-app-action-sidebar-thread-id]')")
            for sample in samples:
                selector = '[data-app-action-sidebar-thread-id="local:' + sample['id'] + '"]'
                evaluate(f'document.querySelector({json.dumps(selector)}).click()')
                wait_for("!!document.querySelector('[data-markdown-text-style=assistant-message]')")
                time.sleep(1)
                # Use the actual resize handle; message CSS stays untouched.
                align_sidebar()
                evaluate("document.querySelectorAll('[aria-label=\"显示/隐藏侧边面板\"][aria-expanded=true]').forEach(e=>e.click())")
                for state in (['bottom', 'scroll-600', 'activity', 'tool', 'command'] if args.include_upper else ['bottom', 'activity', 'tool', 'command']):
                    if state == 'scroll-600':
                        evaluate("document.querySelector('.thread-scroll-container').scrollTop=-600")
                    elif state == 'command':
                        found = evaluate("""(() => {
                            const work=[...document.querySelectorAll('.thread-scroll-container button')].filter(e=>e.innerText.startsWith('用时')).at(-1);
                            if(!work)return false;
                            const button=[...work.parentElement.parentElement.querySelectorAll('button')].find(e=>e.parentElement.className.includes('activity-header') && /^已运行/.test(e.parentElement.innerText));
                            if(!button)return false;button.click();button.scrollIntoView({block:'start'});return true;
                        })()""")
                        if not found:
                            continue
                        evaluate("document.querySelector('.thread-scroll-container').scrollTop-=90")
                    elif state == 'tool':
                        found = evaluate("""(() => {
                            const work=[...document.querySelectorAll('.thread-scroll-container button')].filter(e=>e.innerText.startsWith('用时')).at(-1);
                            if(!work)return false;
                            const button=[...work.parentElement.parentElement.querySelectorAll('button[aria-expanded=false]')].find(e=>e!==work && /运行|探索|读取|工具/.test(e.innerText));
                            if(!button)return false; button.click(); button.scrollIntoView({block:'start'});return true;
                        })()""")
                        if not found:
                            continue
                        evaluate("document.querySelector('.thread-scroll-container').scrollTop-=90")
                    elif state == 'activity':
                        found = evaluate("(() => {const b=[...document.querySelectorAll('.thread-scroll-container button')].filter(e=>e.innerText.startsWith('用时')).at(-1);if(!b)return false;if(b.getAttribute('aria-expanded')==='false')b.click();return true})()")
                        if not found:
                            continue
                        time.sleep(.4)
                        evaluate("[...document.querySelectorAll('.thread-scroll-container button')].filter(e=>e.innerText.startsWith('用时')).at(-1).scrollIntoView({block:'start'})")
                        evaluate("document.querySelector('.thread-scroll-container').scrollTop-=90")
                    else:
                        evaluate("document.querySelectorAll('.thread-scroll-container button[aria-expanded=true]').forEach(e=>{if(e.innerText.startsWith('用时'))e.click()})")
                        time.sleep(.4)
                        evaluate("(() => {const s=document.querySelector('.thread-scroll-container'); s.scrollTop=getComputedStyle(s).flexDirection==='column-reverse'?0:s.scrollHeight})()")
                    time.sleep(.4)
                    if state == 'bottom':
                        previous_height = None
                        stable = 0
                        for _ in range(60):
                            height = evaluate("(() => {const s=document.querySelector('.thread-scroll-container');s.scrollTop=getComputedStyle(s).flexDirection==='column-reverse'?0:s.scrollHeight;return s.scrollHeight})()")
                            stable = stable + 1 if height == previous_height else 0
                            previous_height = height
                            time.sleep(.2)
                            if stable >= 5:
                                break
                    prefix = args.output / 'reference' / f"{sample['slug']}-{theme}-{state}"
                    prefix.parent.mkdir(parents=True, exist_ok=True)
                    prefix.with_suffix('.json').write_text(json.dumps(evaluate(STYLE_PROBE),ensure_ascii=False,indent=2))
                    prefix.with_suffix('.html').write_text(evaluate("document.querySelector('.thread-scroll-container').outerHTML"))
                    png = cdp.call('Page.captureScreenshot', {'format':'png','captureBeyondViewport':False})['data']
                    prefix.with_suffix('.png').write_bytes(base64.b64decode(png))
                    print(prefix, flush=True)
    finally:
        cdp.close()


if __name__ == '__main__':
    main()
