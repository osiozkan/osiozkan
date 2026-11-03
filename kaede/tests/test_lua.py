import os, textwrap
import pytest
from kaede.core.ass import Document
from kaede.lua.engine import Engine, Host, Cancelled, AutomationError


DOC = Document.parse(
    "[Script Info]\nPlayResX: 1280\nPlayResY: 720\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize\n"
    "Style: Default,Arial,40\n\n[Events]\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Merhaba\n"
    "Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,{\\k50}Ka{\\k50}ra{\\k100}oke\n"
    "Dialogue: 0,0:00:07.00,0:00:08.00,Default,,0,0,0,,Üçüncü\n")


def run(tmp_path, code, doc=DOC, sel=(0,), active=0, which=0):
    p = tmp_path / "s.lua"
    p.write_text(textwrap.dedent(code), encoding="utf-8")
    eng = Engine(Host())
    s = eng.load_script(str(p))
    assert not s.error, s.error
    return eng, s, eng.run_macro(s.macros[which], doc, list(sel), active)


def test_shift_and_selection(tmp_path):
    eng, s, r = run(tmp_path, '''
        script_name = "Shift"; script_description = "d"; script_author = "me"; script_version = "1"
        function f(subs, sel, act)
          for _, i in ipairs(sel) do
            local l = subs[i]
            l.start_time = l.start_time + 500
            l.end_time = l.end_time + 500
            l.text = l.text .. " !"
            subs[i] = l
          end
          return sel, sel[1]
        end
        aegisub.register_macro("Shift", "x", f)
    ''', sel=(0, 2))
    assert s.name == "Shift" and s.author == "me"
    ev = r.doc.events
    assert ev[0].start == 1500 and ev[0].text == "Merhaba !"
    assert ev[1].start == 4000
    assert ev[2].start == 7500 and ev[2].text.endswith("!")
    assert r.selection == [0, 2] and r.active == 0


def test_insert_delete_append(tmp_path):
    eng, s, r = run(tmp_path, '''
        function f(subs, sel)
          local l = subs[#subs]
          l.text = "new"
          subs.append(l)
          subs.insert(#subs - 1, l)
          subs.delete(#subs)
          subs.deleterange(1, 1) -- would delete first info line
          aegisub.log("hello %d", 5)
          return {#subs}
        end
        aegisub.register_macro("T", "x", f)
    ''')
    assert len(r.doc.events) == 4
    assert r.logs == ["hello 5"]
    assert r.doc.get_info("PlayResX") == ""  # first info line removed


def test_cancel_and_error(tmp_path):
    p = tmp_path / "c.lua"
    p.write_text('aegisub.register_macro("C","",function() aegisub.cancel() end)\n'
                 'aegisub.register_macro("E","",function() error("boom") end)')
    eng = Engine()
    s = eng.load_script(str(p))
    with pytest.raises(Cancelled):
        eng.run_macro(s.macros[0], DOC, [0])
    with pytest.raises(AutomationError) as e:
        eng.run_macro(s.macros[1], DOC, [0])
    assert "boom" in str(e.value)


def test_libs(tmp_path):
    eng, s, r = run(tmp_path, '''
        local util = require "util"
        local utf8 = require "utf8"
        local ffi = require "ffi"
        local bit = require "bit"
        include("karaskel.lua")
        function f(subs, sel)
          local meta, styles = karaskel.collect_head(subs)
          local l = subs[sel[1]]
          karaskel.preproc_line(subs, meta, styles, l)
          assert(meta.res_x == 1280 and styles.Default and l.styleref.name == "Default")
          assert(l.halign == "center" and l.valign == "bottom")
          assert(l.x == 640 and l.y == 710, l.x .. "," .. l.y)
          assert(bit.band(5, 3) == 1)
          assert(utf8.len("Üçüncü") == 6 and utf8.sub("Üçüncü", 1, 2) == "Üç")
          assert(util.ass_color(255, 128, 0) == "&H0080FF&")
          local m = re.match("abc123", "([a-z]+)(\\\\d+)")
          assert(m[2].str == "abc" and m[3].str == "123")
          assert(re.sub("a-b-c", "-", "+") == "a+b+c")
          local o = re.compile("(\\\\w+)@"):match("bob@x")
          assert(o[2].str == "bob")
          assert(unicode.len("Üçü") == 3)
          for c in unicode.chars("aÜ") do l.text = l.text .. c end
          subs[sel[1]] = l
        end
        aegisub.register_macro("L", "", f)
    ''')
    assert r.doc.events[0].text == "MerhabaaÜ"


def test_karaoke_parse(tmp_path):
    eng, s, r = run(tmp_path, '''
        include("karaskel.lua")
        function f(subs, sel)
          local meta, styles = karaskel.collect_head(subs)
          local l = subs[sel[1]]
          karaskel.preproc_line(subs, meta, styles, l)
          assert(l.kara.n == 3, "n=" .. tostring(l.kara.n))
          assert(l.kara[1].text_stripped == "Ka" and l.kara[1].duration == 500)
          assert(l.kara[3].start_time == 1000 and l.kara[3].end_time == 2000)
        end
        aegisub.register_macro("K", "", f)
    ''', sel=(1,))


def test_dialog_headless(tmp_path):
    eng, s, r = run(tmp_path, '''
        function f(subs, sel)
          local b, res = aegisub.dialog.display({
            {class="label", label="x", x=0, y=0},
            {class="intedit", name="n", value=7, x=0, y=1},
            {class="checkbox", name="c", value=true, x=0, y=2},
            {class="dropdown", name="d", items={"a","b"}, value="b", x=0,y=3}})
          assert(b == true and res.n == 7 and res.c == true and res.d == "b")
        end
        aegisub.register_macro("D", "", f)
    ''')


def test_validate_and_filter(tmp_path):
    p = tmp_path / "v.lua"
    p.write_text('''
      aegisub.register_macro("V","",function() end, function(subs, sel) return #sel > 1 end)
      aegisub.register_filter("F","",5,function(subs, cfg)
        for i=1,#subs do local l=subs[i]; if l.class=="dialogue" then l.text="F"; subs[i]=l end end end)
    ''')
    eng = Engine(); s = eng.load_script(str(p))
    assert not eng.validate(s.macros[0], DOC, [0], 0)
    assert eng.validate(s.macros[0], DOC, [0, 1], 0)
    r = eng.run_filter(s.filters[0], DOC)
    assert all(e.text == "F" for e in r.doc.events)


def test_lfs_utils_clipboard(tmp_path):
    eng, s, r = run(tmp_path, '''
        local lfs = require "lfs"
        require "utils"
        local clip = require "clipboard"
        function f(subs, sel)
          assert(lfs.attributes(lfs.currentdir(), "mode") == "directory")
          assert(("  x  "):trim() == "x")
          assert(math.round(2.5) == 3)
          clip.set("abc"); assert(clip.get() == "abc")
          local d = aegisub.decode_path("?script")
          assert(type(d) == "string")
        end
        aegisub.register_macro("U", "", f)
    ''')


def test_script_dir_include(tmp_path):
    (tmp_path / "mylib.lua").write_text("local M = {} function M.hi() return 'hi' end return M")
    eng, s, r = run(tmp_path, '''
        local m = require "mylib"
        function f(subs, sel) local l = subs[sel[1]]; l.text = m.hi(); subs[sel[1]] = l end
        aegisub.register_macro("I", "", f)
    ''')
    assert r.doc.events[0].text == "hi"
