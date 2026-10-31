import os
import pytest
from yorusub.core import AssFile
from yorusub.lua import LuaRuntime

D = os.path.join(os.path.dirname(__file__), "data")


@pytest.fixture
def env():
    rt = LuaRuntime(log=lambda s: None)
    ms = {m.name: m for m in rt.load_script(os.path.join(D, "hello.lua"))}
    return rt, ms, AssFile.load(os.path.join(D, "sample.ass"))


def test_macros_registered(env):
    rt, ms, _ = env
    assert set(ms) >= {"Büyük harf", "Ekle", "Karaoke", "Diyalog", "Regex", "Sil"}


def test_modify_lines(env):
    rt, ms, a = env
    out, sel, _ = rt.run(ms["Büyük harf"], a, [0])
    assert out.events[0].text.startswith("MERHABA")
    assert out.events[1].text == a.events[1].text        # seçilmeyen değişmez
    assert out.styles[1].name == "Sign" and out.get_info("PlayResX") == "1920"
    assert sel == [0]
    assert a.events[0].text.startswith("Merhaba")         # girdi mutasyona uğramaz


def test_append_delete(env):
    rt, ms, a = env
    out, _, _ = rt.run(ms["Ekle"], a, [0])
    assert len(out.events) == 5 and out.events[-1].text == "eklendi" and out.events[-1].start == 9000
    out, _, _ = rt.run(ms["Sil"], a, [1])
    assert len(out.events) == 3


def test_karaskel(env):
    rt, ms, a = env
    out, _, _ = rt.run(ms["Karaoke"], a, [2])
    assert out.events[2].effect.startswith("syl=4;w=")


def test_dialog_and_re(env):
    rt, ms, a = env
    out, _, _ = rt.run(ms["Diyalog"], a, [0])
    assert out.events[0].actor == "Tamam:42"
    out, _, _ = rt.run(ms["Regex"], a, [0])
    assert out.events[0].text.startswith("dünya Merhaba")


def test_error_reported():
    rt = LuaRuntime(log=lambda s: None)
    from yorusub.lua import LuaScriptError
    with pytest.raises(LuaScriptError):
        rt.load_source("error('boom')")
