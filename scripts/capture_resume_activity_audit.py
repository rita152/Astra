#!/usr/bin/env python3
"""Capture every activity group of the open resumed thread through real CDP.

Use a dedicated Electron debug profile. Screenshots cover each turn and selected
computer-use disclosures; JSON records all groups/items in both themes.
"""
import argparse
import base64
import json
from pathlib import Path
import time
import urllib.request
from cdp_chatgpt import Cdp
from capture_resume_reference import STYLE_PROBE


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--endpoint', default='http://127.0.0.1:9222')
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args(); a.output.mkdir(parents=True, exist_ok=True)
    target = next(t for t in json.load(urllib.request.urlopen(a.endpoint+'/json/list')) if t['url']=='app://-/index.html')
    c = Cdp(target['webSocketDebuggerUrl'])
    def ev(s):
        r=c.call('Runtime.evaluate',{'expression':s,'returnByValue':True,'awaitPromise':True})
        if 'exceptionDetails' in r: raise RuntimeError(r['exceptionDetails'])
        return r.get('result',{}).get('value')
    def click(selector):
        rect=ev(f'document.querySelector({json.dumps(selector)}).getBoundingClientRect().toJSON()')
        for kind in ['mousePressed','mouseReleased']:
            c.call('Input.dispatchMouseEvent',{'type':kind,'x':rect['x']+rect['width']/2,'y':rect['y']+rect['height']/2,'button':'left','clickCount':1})
    def save(name):
        time.sleep(.5)
        (a.output/(name+'.png')).write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
        (a.output/(name+'.json')).write_text(json.dumps(ev(STYLE_PROBE),ensure_ascii=False,indent=2))
    try:
        for theme,label in [('light','浅色'),('dark','深色')]:
            click('[aria-label="打开个人资料菜单"]');time.sleep(.5)
            ev('[...document.querySelectorAll("[role=menuitem]")].find(e=>e.innerText.includes("设置")).click()');time.sleep(.4)
            ev('[...document.querySelectorAll("button")].find(e=>e.innerText==="外观").click()');time.sleep(.3)
            ev(f'document.querySelector(\'input[name=appearance-theme][aria-label="{label}"]\').click()');time.sleep(.4)
            ev('[...document.querySelectorAll("button")].find(e=>e.innerText==="返回应用").click()');time.sleep(.5)
            ev('[...document.querySelectorAll("button[aria-expanded=false]")].filter(e=>e.innerText.startsWith("用时")).forEach(e=>e.click())');time.sleep(.5)
            # Close only nested activity disclosures, leaving all turn bodies open.
            ev('[...document.querySelectorAll(".thread-scroll-container button[aria-expanded=true]")].filter(e=>!e.innerText.startsWith("用时")&&!e.innerText.startsWith("再显示")).forEach(e=>e.click())');time.sleep(.5)
            for index in range(5):
                ev(f'[...document.querySelectorAll(".thread-scroll-container button")].filter(e=>e.innerText.startsWith("用时"))[{index}].scrollIntoView({{block:"start"}})')
                ev('document.querySelector(".thread-scroll-container").scrollTop-=90');save(f'{theme}-turn-{index+1}')
            ev('[...document.querySelectorAll("button[aria-expanded=false]")].filter(e=>e.classList.contains("group/activity-header")).forEach(e=>e.click())');time.sleep(.6)
            probe=ev('''(()=>{const r=document.querySelector('.thread-scroll-container');return {groups:[...r.querySelectorAll('[data-local-conversation-item-target-ids]')].map(e=>({ids:e.getAttribute('data-local-conversation-item-target-ids').split(' ').map(decodeURIComponent),title:e.innerText.split('\\n')[0],text:e.innerText,rows:[...e.querySelectorAll('[aria-labelledby]')].map(b=>({title:document.getElementById(b.getAttribute('aria-labelledby'))?.innerText,expanded:b.getAttribute('aria-expanded')}))})),messages:[...r.querySelectorAll('[data-response-annotation-target],[data-local-conversation-user-anchor],[data-markdown-text-tone=user-message]')].map(e=>({attrs:Object.fromEntries([...e.attributes].filter(a=>a.name.startsWith('data-')).map(a=>[a.name,a.value])),text:e.innerText})),work:[...r.querySelectorAll('button')].filter(e=>e.innerText.startsWith('用时')).map(e=>e.innerText)}})()''')
            (a.output/f'{theme}-dom-audit.json').write_text(json.dumps({'value':probe},ensure_ascii=False,indent=2))
            for name,title in [('browser','枚举应用以准备独立 GPUI 界面验收'),('app','连接独立 GPUI Capture'),('failure','关闭修复前专用验收窗口，准备更新可执行文件')]:
                selector=f'[...document.querySelectorAll("button[aria-labelledby]")].find(b=>document.getElementById(b.getAttribute("aria-labelledby"))?.innerText==={json.dumps(title)})'
                ev(selector+'.scrollIntoView({block:"start"})');ev('document.querySelector(".thread-scroll-container").scrollTop-=90');save(f'{theme}-{name}-row')
                ev(selector+'.click()');time.sleep(.5);ev(selector+'.scrollIntoView({block:"start"})');ev('document.querySelector(".thread-scroll-container").scrollTop-=90');save(f'{theme}-{name}-detail')
                ev(selector+'.click()');time.sleep(.3)
    finally:c.close()


if __name__=='__main__':main()
