from subforge.core import parse_ass, write_ass
from subforge.core.formats import parse_srt, parse_vtt, parse_lrc, parse_microdvd, load_file, detect_format
from subforge.core.io import decode_bytes

TRICKY = ("﻿[Script Info]\r\n; comment line\r\nTitle: A: B\r\nScriptType: v4.00+\r\nPlayResX: 640\r\n\r\n"
          "[Aegisub Project Garbage]\r\nAudio File: ?video\r\nVideo File: x.mkv\r\nVideo Position: 120\r\n\r\n"
          "[V4+ Styles]\r\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\r\n"
          "Style: Default,Arial,20,&HFFFFFF&,&H000000FF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\r\n"
          "Style: Alt Style ,Comic Sans MS,18,16777215,255,0,0,0,0,0,0,100,100,0,0,1,2,2,8,10,10,10,0\r\n\r\n"
          "[Events]\r\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\r\n"
          "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0000,0000,0000,,a,b,c\r\n"
          "Dialogue: 0,0:00:02.00,0:00:03.00,Default\r\n"
          "Comment: 0,0:00:03.00,0:00:04.00,Default,x,0,0,0,fx,{\\k5}z\r\n"
          "Garbage line without colon\r\n\r\n"
          "[Fonts]\r\nfontname: a_0.ttf\r\n!!!!\r\n")


def test_tricky_file_survives():
    d = parse_ass(TRICKY)
    ev = d.events
    assert ev[0].text == "a,b,c" and ev[0].margin_l == 0
    assert ev[1].text == "" and ev[1].style == "Default"          # truncated line: kept, not lost
    assert ev[2].comment and ev[2].effect == "fx"
    assert d.get_info("Title") == "A: B"
    assert d.styles[0].color1 == "&H00FFFFFF" and d.styles[0].bold and d.styles[1].name == "Alt Style"
    assert d.styles[1].color1 == "&H00FFFFFF"                      # decimal colour
    out = write_ass(d)
    assert "Audio File: ?video" in out and "[Fonts]" in out and "fontname: a_0.ttf" in out and "; comment line" in out
    d2 = parse_ass(out)
    assert len(d2.events) == 3 and d2.get_info("Title") == "A: B"


def test_empty_and_header_only():
    assert parse_ass("").styles and parse_ass("[Script Info]\nTitle: x\n").events == []
    assert "[Events]" in write_ass(parse_ass(""))


def test_srt_edge_cases():
    s = "﻿1\r\n00:00:01,5 --> 00:00:02,25\r\nA\r\n\r\n\r\n3\r\n00:00:03.000 --> 00:00:04.000\r\nB\r\nC\r\n\r\nnonsense\r\n"
    d = parse_srt(s)
    assert [(e.start_time, e.end_time, e.text) for e in d.events] == [(1500, 2250, "A"), (3000, 4000, "B\\NC")]


def test_other_formats():
    v = parse_vtt("WEBVTT\n\nNOTE x\n\ncue1\n00:00:01.000 --> 00:00:02.000 align:start\n<v Bob><c.red>Hi</c> <00:00:01.500>there\n")
    assert v.events[0].text == "Hi there"
    l = parse_lrc("[00:01.50]one\n[00:03.00]two\n")
    assert [(e.start_time, e.end_time) for e in l.events] == [(1500, 3000), (3000, 7000)]
    m = parse_microdvd("{1}{1}25\n{25}{50}hello|world\n")
    assert m.events[0].start_time == 1000 and m.events[0].text == "hello\\Nworld"
    assert detect_format("x.txt", "{1}{2}a") == "microdvd"


def test_encoding_detection():
    tr = "Çocuk İstanbul'a gitti, ışık öğle şimdi"
    assert decode_bytes(tr.encode("cp1254"))[0] == tr
    assert decode_bytes(tr.encode("utf-8"))[1] == "utf-8"
    assert decode_bytes(tr.encode("utf-16"))[0] == tr
    assert decode_bytes(b"\xef\xbb\xbf" + tr.encode("utf-8"))[0] == tr
