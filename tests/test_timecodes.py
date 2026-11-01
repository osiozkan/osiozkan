from subforge.core.timecodes import Timecodes


def test_cfr():
    t = Timecodes.cfr(24000 / 1001)
    assert t.time_of(0) == 0 and t.time_of(24) == 1001
    assert t.frame_at(1001) == 24 and t.frame_at(1000) == 23
    assert t.start_ms_for_frame(24) == int((t.time_of(23) + t.time_of(24)) / 2 + 0.5)


def test_v2_vfr():
    t = Timecodes.from_v2("# timecode format v2\n0\n100\n200\n400\n600\n")
    assert t.is_vfr and t.frame_at(399) == 2 and t.frame_at(400) == 3 and t.frame_at(10_000) > 4
    assert t.time_of(3) == 400


def test_v1():
    t = Timecodes.from_v1("# timecode format v1\nAssume 10\n2,3,20\n")
    assert t.time_of(1) == 100 and t.time_of(3) == 100 + 100 + 50 and t.time_of(4) == 100 + 100 + 50 + 50
