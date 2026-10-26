import { test } from "node:test";
import assert from "node:assert/strict";
import { parse } from "../src/core/index.js";
import { AutomationHost } from "../src/lua/host.js";

const base = () => parse(`[Script Info]
PlayResX: 1920
PlayResY: 1080

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,one
Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,two
Dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,three
`);

function run(src, opts = {}, name) {
  const doc = base();
  const h = new AutomationHost(doc, opts);
  const info = h.loadScript(src, "/tmp/test.lua");
  const res = h.runMacro(name ?? info.macros[0], opts.run);
  return { doc, h, res, info };
}

test("registers macros and mutates lines", () => {
  const { doc, info } = run(`
    script_name="T"
    aegisub.register_macro("M", "d", function(subs, sel)
      for _, i in ipairs(sel) do local l = subs[i]; l.text = l.text:upper(); subs[i] = l end
    end)`);
  assert.deepEqual(info.macros, ["M"]);
  assert.deepEqual(doc.events.map((e) => e.text), ["ONE", "TWO", "THREE"]);
});

test("selection is respected and returned", () => {
  const { doc, res } = run(`aegisub.register_macro("M","",function(subs,sel,act)
    local l = subs[sel[1]]; l.text="X"; subs[sel[1]]=l; return {sel[1]}, sel[1] end)`, { run: { selected: [1] } });
  assert.deepEqual(doc.events.map((e) => e.text), ["one", "X", "three"]);
  assert.deepEqual(res.selected, [1]);
});

test("insert/delete/append use dot calls", () => {
  const { doc } = run(`aegisub.register_macro("M","",function(subs)
    local first
    for i=1,#subs do if subs[i].class=="dialogue" then first=i break end end
    local l = util.copy(subs[first]); l.text="ins"
    subs.insert(first, l)
    subs.delete(first+2)
    subs.append(l)
  end)`);
  assert.deepEqual(doc.events.map((e) => e.text), ["ins", "one", "three", "ins"]);
});

test("LuaJIT compat: unpack, loadstring, %d with floats, no .0 in concat", () => {
  const { doc } = run(`aegisub.register_macro("M","",function(subs)
    local a, b = unpack({1, 2})
    local f = loadstring("return 7")
    local l = subs[#subs]
    l.text = string.format("%d", 3.7) .. "|" .. (960/2) .. "|" .. a .. b .. f() .. "|" .. math.floor(2.5)
    subs[#subs] = l
  end)`);
  assert.equal(doc.events[2].text, "3|480|127|2");
});

test("karaskel + text_extents", () => {
  const { doc } = run(`include("karaskel.lua")
    aegisub.register_macro("M","",function(subs, sel)
      local meta, styles = karaskel.collect_head(subs)
      local l = subs[sel[1]]
      karaskel.preproc_line(subs, meta, styles, l)
      l.text = ("%d %d %s"):format(meta.res_x, l.width > 0 and 1 or 0, l.styleref.name)
      subs[sel[1]] = l
    end)`, { run: { selected: [0] } });
  assert.equal(doc.events[0].text, "1920 1 Default");
});

test("utils: colors, interpolation", () => {
  const { doc } = run(`include("utils.lua")
    aegisub.register_macro("M","",function(subs, sel)
      local l = subs[sel[1]]
      l.text = util.ass_color(255,0,16) .. util.interpolate_color(0.5, "&H000000&", "&HFFFFFF&") .. util.ass_alpha(128)
      subs[sel[1]] = l
    end)`, { run: { selected: [0] } });
  assert.equal(doc.events[0].text, "&H1000FF&&H808080&&H80&");
});

test("dialog returns defaults and honours overrides", () => {
  const src = `aegisub.register_macro("M","",function(subs, sel)
    local btn, r = aegisub.dialog.display({
      {class="label", label="n", x=0,y=0},
      {class="intedit", name="n", value=5, x=0,y=1},
      {class="checkbox", name="c", value=false, label="c", x=0,y=2},
      {class="edit", name="t", value="d", x=0,y=3}})
    local l = subs[sel[1]]
    l.text = tostring(btn) .. "," .. r.n .. "," .. tostring(r.c) .. "," .. r.t
    subs[sel[1]] = l
  end)`;
  const a = run(src, { run: { selected: [0] } });
  assert.equal(a.doc.events[0].text, "true,5,false,d");
  const b = run(src, { run: { selected: [0] }, dialog: { values: { n: "9", c: "true", t: "z" } } });
  assert.equal(b.doc.events[0].text, "true,9,true,z");
});

test("aegisub.cancel and errors", () => {
  const { res } = run(`aegisub.register_macro("M","",function() aegisub.cancel() end)`);
  assert.equal(res.cancelled, true);
  assert.throws(() => run(`aegisub.register_macro("M","",function() error("boom") end)`), /boom/);
});

test("re module and logging", () => {
  const { doc, h } = run(`local re = require("aegisub.re")
    aegisub.register_macro("M","",function(subs, sel)
      local l = subs[sel[1]]
      l.text = re.sub("hello world", "(\\\\w+) (\\\\w+)", "\\\\2 \\\\1")
      aegisub.log("hi %s\\n", "there")
      subs[sel[1]] = l
    end)`, { run: { selected: [0] } });
  assert.equal(doc.events[0].text, "world hello");
  assert.equal(h.logs[0].message, "hi there\n");
});

test("io/os: write, append, read back, lines, remove (utf-8 safe)", async () => {
  const { tmpdir } = await import("node:os");
  const p = tmpdir() + "/osisub-io-test.txt";
  const { doc } = run(`aegisub.register_macro("M","",function(subs, sel)
    local f = assert(io.open(${JSON.stringify(p)}, "w")); f:write("satır1\\n", 42, "\\n"); f:close()
    f = io.open(${JSON.stringify(p)}, "a"); f:write("ğüş"); f:close()
    local out = {}
    for l in io.lines(${JSON.stringify(p)}) do out[#out+1] = l end
    local g = io.open(${JSON.stringify(p)}, "r"); local all = g:read("*a"); g:close()
    local missing = io.open("/definitely/not/here", "r")
    local l = subs[sel[1]]
    l.text = table.concat(out, "|") .. "#" .. #all .. "#" .. tostring(missing) .. "#" .. tostring(os.remove(${JSON.stringify(p)}))
    subs[sel[1]] = l
  end)`, { run: { selected: [0] } });
  assert.equal(doc.events[0].text, "satır1|42|ğüş#" + Buffer.byteLength("satır1\n42\nğüş") + "#nil#true");
});
