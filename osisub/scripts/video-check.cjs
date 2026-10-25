const { chromium } = require('playwright'); const fs = require('node:fs');
(async () => {
const b = await chromium.launch({ channel: 'chromium', args: ['--autoplay-policy=no-user-gesture-required','--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist'] }); const p = await b.newPage({ viewport: { width: 1400, height: 800 } });
const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => ['error','warning'].includes(m.type()) && errs.push(m.text()));
p.on('worker', w => { console.log('worker', w.url()); w.on('console', m => console.log('W:', m.text())); });
await p.goto('http://localhost:8765/');
await p.evaluate(t => window.OsiSub.loadSubtitleText(t, 'sample.ass'), fs.readFileSync('/home/user/osiozkan/osisub/examples/sample.ass', 'utf8'));
const [fc] = await Promise.all([p.waitForEvent('filechooser'), p.evaluate(() => { window.OsiSub.actions['open-video'](); })]); await fc.setFiles(process.argv[2] /* path to a test video, e.g. ffmpeg -f lavfi -i color=c=0x203040:s=1280x720:d=12 test.webm */);
await p.waitForTimeout(3000);
await p.evaluate(() => { const v = document.querySelector('#video'); v.currentTime = 2; });
await p.waitForTimeout(500); console.log('draw', await p.evaluate(async () => { const j = window.OsiSub.jassub(); try { return String(await j.renderer._draw(2.0, true)); } catch(e) { return 'ERR '+e.message; } }));
await p.waitForTimeout(2500);
console.log(await p.evaluate(() => { const v=document.querySelector('#video'); return JSON.stringify({rs:v.readyState,err:v.error&&v.error.message,t:v.currentTime,dur:v.duration,ov:document.querySelector('#overlay').hidden,cv:[0]}); }));
console.log(await p.evaluate(async () => { const j = window.OsiSub.jassub(); if(!j) return 'no jassub'; const ev = await j.renderer.getEvents(); const st = await j.renderer.getStyles(); return 'events '+ev.length+' styles '+st.length+' busy '+j.busy+' size '+j._videoWidth+'x'+j._videoHeight+' skipped '+j._skipped; }));
console.log(await p.evaluate(() => [...document.querySelectorAll('#videoWrap canvas')].map(c=>c.id+':'+c.width+'x'+c.height+' '+JSON.stringify(c.getBoundingClientRect())+' '+c.style.cssText).join('\n')));
await p.screenshot({ path: '/tmp/video.png' });
console.log('errors:', errs.slice(0,5)); await b.close();
})();
