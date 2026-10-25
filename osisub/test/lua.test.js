import test from 'node:test';
import assert from 'node:assert/strict';
import { LuaScript } from '../src/core/lua.js';
import { parseAss, writeAss, newScript, defaultDialogue } from '../src/core/ass.js';

const base = () => { const l = newScript(); l.push(defaultDialogue({ text: 'Hello', start_time: 1000, end_time: 3000 })); l.push(defaultDialogue({ text: 'World', start_time: 3000, end_time: 4000 })); return l; };

test('macro registration + modifying lines', async () => {
  const s = new LuaScript(`
script_name = "Upper"
script_description = "d"
function go(subs, sel)
  for i = 1, #subs do
    local l = subs[i]
    if l.class == "dialogue" then l.text = l.text:upper(); subs[i] = l end
  end
  return sel
end
aegisub.register_macro("Upper", "x", go)`);
  assert.equal(s.info.name, 'Upper'); assert.equal(s.macros.length, 1);
  const r = await s.run('macro', 0, base(), [], 0);
  assert.deepEqual(r.lines.filter(l => l.class === 'dialogue').map(l => l.text), ['HELLO', 'WORLD']);
  assert.ok(writeAss(r.lines).includes('HELLO'));
});

test('insert/delete/append and dialogs via coroutine', async () => {
  const s = new LuaScript(`
include("utils.lua")
aegisub.register_macro("T", "", function(subs, sel)
  local btn, res = aegisub.dialog.display({{class="label", label="x", x=0,y=0}, {class="intedit", name="n", value=2, x=0,y=1}}, {"Go","Stop"})
  if btn ~= "Go" then aegisub.cancel() end
  local last
  for i = 1, #subs do if subs[i].class == "dialogue" then last = i end end
  for k = 1, res.n do
    local l = util.copy(subs[last])
    l.text = "dup" .. k; subs.insert(last + k, l)
  end
end)`);
  const r = await s.run('macro', 0, base(), [], 0, async req => ({ button: 'Go', values: { n: req.dialog[1].value } }));
  assert.equal(r.lines.filter(l => l.class === 'dialogue').length, 4);
  const c = await s.run('macro', 0, base(), [], 0, async () => ({ cancelled: true }));
  assert.ok(c.cancelled);
});

test('karaskel + utils + re + unicode', async () => {
  const s = new LuaScript(`
include("karaskel.lua")
local re = require "aegisub.re"
include("unicode.lua")
aegisub.register_macro("K", "", function(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local i = 0
  for j = 1, #subs do
    local l = subs[j]
    if l.class == "dialogue" then
      karaskel.preproc_line(subs, meta, styles, l)
      l.effect = tostring(l.kara.n) .. "/" .. tostring(meta.res_x) .. "/" .. unicode.len("かな") .. "/" .. re.sub("l+", "L", "hello") .. "/" .. util.ass_color(255,0,0)
      subs[j] = l
    end
  end
end)`);
  const lines = newScript(); lines.push(defaultDialogue({ text: '{\\k50}ka{\\k30}ra{\\k20}oke', end_time: 1000 }));
  const r = await s.run('macro', 0, lines);
  assert.equal(r.lines.find(l => l.class === 'dialogue').effect, '2/1920/2/heLo/&H0000FF&');
});

test('error reporting', async () => {
  const s = new LuaScript(`aegisub.register_macro("E","",function() error("boom") end)`);
  await assert.rejects(() => s.run('macro', 0, base()), /boom/);
});

import fs from 'node:fs';
test('Aegisub kara-templater.lua (original) runs against real template lines', async () => {
  const src = fs.readFileSync(new URL('../third_party/aegisub-lua/kara-templater.lua', import.meta.url), 'utf8');
  const s = new LuaScript(src, 'kara-templater.lua');
  assert.ok(s.macros.length >= 1, 'templater macro registered');
  const lines = newScript();
  lines.push(defaultDialogue({ comment: true, effect: 'template syl', text: '!retime("line",-100,100)!{\\pos($x,$y)\\fad(100,0)}', style: 'Default', start_time: 0, end_time: 3000 }));
  lines.push(defaultDialogue({ comment: false, effect: 'karaoke', style: 'Default', start_time: 1000, end_time: 3000, text: '{\\k50}ka{\\k30}ra{\\k20}oke' }));
  const r = await s.run('macro', 0, lines, [], 0);
  const out = r.lines.filter(l => l.class === 'dialogue' && !l.comment);
  assert.ok(out.length >= 3, `expected fx lines, got ${out.length}: ${r.logs.join('')}`);
  assert.ok(out.every(l => /\\pos\(/.test(l.text)), out.map(l => l.text).join('|'));
});

test('text_extents uses real font metrics and 5.1 compat shims work', async () => {
  const s = new LuaScript(`
aegisub.register_macro("M", "", function(subs)
  local st = subs[2]
  local w, h, d = aegisub.text_extents(st, "Hello")
  local f = loadstring("return 40 + 2")
  local out = string.format("%d|%s|%d|%d", 7.9, tostring(unpack({1, 2})), f(), bit.band(0xFF, 0x0F))
  subs[1] = {class = "info", key = "T", value = out .. "|" .. tostring(w > 50 and w < 400) .. "|" .. tostring(h)}
end)`);
  const r = await s.run('macro', 0, newScript());
  const t = r.lines.find(l => l.class === 'info' && l.key === 'T');
  assert.match(t.value, /^7\|1\|42\|15\|true\|/);
});
