const { chromium } = require('playwright'); const fs = require('node:fs');(async()=>{
const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1400, height: 800 } });
const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
await p.goto('http://localhost:8765/');
await p.evaluate(t => window.OsiSub.loadSubtitleText(t, 'sample.ass'), fs.readFileSync('' + __dirname + '/../examples/sample.ass', 'utf8'));
await p.evaluate(t => window.OsiSub.loadLua(t, 'add_prefix.lua'), fs.readFileSync('' + __dirname + '/../examples/add_prefix.lua', 'utf8'));
await p.locator('.grow').first().click();
const run = p.evaluate(() => window.OsiSub.runMacro(0, 0, 'macro'));
await p.waitForSelector('#modal:not([hidden]) input[name=p]');
await p.screenshot({ path: '/tmp/dialog.png' });
await p.fill('input[name=p]', '>> '); await p.click('button[data-b=Apply]'); const t0=Date.now(); await run; console.log('run took',Date.now()-t0); console.log("errs so far", errs);
console.log('row1:', await p.locator('.grow .txt').first().textContent());
p.evaluate(() => window.OsiSub.actions.qc()); await p.waitForSelector('.issue'); console.log('issues:', await p.locator('.issue').count());
await p.screenshot({ path: '/tmp/qc.png' }); await p.keyboard.press('Escape');
await p.screenshot({ path: '/tmp/main.png' });
console.log('errors:', errs); await b.close();
})();
