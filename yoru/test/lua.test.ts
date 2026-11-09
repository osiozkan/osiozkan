import { describe, expect, it } from "vitest";
import { LuaRuntime } from "../src/lua/runtime";
import { parseAss } from "../src/core/ass";

const ASS = `[Script Info]
PlayResX: 1280
PlayResY: 720

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\k50}ka{\\k50}ra
Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,second
`;

function setup(script: string) {
  const rt = new LuaRuntime();
  const r = rt.loadScript("t.lua", script);
  if (!r.ok) throw new Error(r.message);
  return { rt, id: r.macros[0].id };
}

describe("lua", () => {
  it("macro edits lines, selection", async () => {
    const { rt, id } = setup(`
script_name = "x"
function m(subs, sel, active)
  for _, i in ipairs(sel) do
    local l = subs[i]
    l.text = l.text:upper() .. "\\\\pos(" .. (1280/2) .. "," .. 10 .. ")"
    l.start_time = l.start_time + 500
    subs[i] = l
  end
  return {sel[1]}
end
aegisub.register_macro("up", "d", m)`);
    const f = parseAss(ASS);
    const res = await rt.run(id, f, [1]);
    expect(res.status).toBe("done");
    expect(res.file!.events[1].text).toBe("SECOND\\pos(640,10)"); // 640 değil 640.0
    expect(res.file!.events[1].start).toBe(4500);
    expect(res.sel).toEqual([1]);
  });
  it("insert/delete/append, error and cancel", async () => {
    const { rt, id } = setup(`
aegisub.register_macro("a", "", function(subs)
  local l = subs[#subs]; l.text = "new"
  subs.append(l); subs.insert(#subs, l); subs.delete(#subs - 2)
end)
aegisub.register_macro("b", "", function(subs) error("boom") end)
aegisub.register_macro("c", "", function(subs) aegisub.cancel() end)`);
    const f = parseAss(ASS);
    const ids = rt.macros.map((m) => m.id);
    const r1 = await rt.run(ids[0], f, []);
    expect(r1.file!.events.map((e) => e.text)).toEqual(["{\\k50}ka{\\k50}ra", "new", "new"]);
    expect((await rt.run(ids[1], f, [])).status).toBe("error");
    expect((await rt.run(ids[2], f, [])).status).toBe("cancelled");
  });
  it("dialog + karaskel + util", async () => {
    const { rt, id } = setup(`
include("karaskel.lua")
include("utils.lua")
aegisub.register_macro("k", "", function(subs, sel)
  local meta, styles = karaskel.collect_head(subs)
  local btn, res = aegisub.dialog.display({{class="label",label="x",x=0,y=0},{class="intedit",name="n",value=7,x=0,y=1}}, {"Go","Cancel"})
  if not btn then aegisub.cancel() end
  local l = subs[4]
  karaskel.preproc_line(subs, meta, styles, l)
  l.text = meta.res_x .. "/" .. l.kara.n .. "/" .. res.n .. "/" .. util.ass_color(255, 0, 16) .. "/" .. l.kara[2].start_time
  subs[4] = l
end)`);
    const res = await rt.run(id, parseAss(ASS), []);
    expect(res.message).toBeUndefined();
    expect(res.file!.events[0].text).toBe("1280/2/7/&H1000FF&/500");
  });
  it("re + bit + unicode", async () => {
    const { rt, id } = setup(`
local bit = require "bit"
aegisub.register_macro("r", "", function(subs)
  local l = subs[4]
  l.text = re.sub(l.text:gsub("{[^}]*}", ""), "(k)a", "\\\\1_") .. bit.band(0xFF, 0x0F) .. unicode.len("şğü")
  subs[4] = l
end)`);
    const res = await rt.run(id, parseAss(ASS), []);
    expect(res.file!.events[0].text).toBe("k_ra153");
  });
});
