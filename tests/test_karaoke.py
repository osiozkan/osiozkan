from subforge.core import karaoke as K


def test_split_modes():
    assert K.split_text("ab c", "char") == ["a", "b ", "c"]
    assert K.split_text("kyou wa", "word") == ["kyou ", "wa"]
    assert "".join(K.split_text("sakura no hana", "romaji")) == "sakura no hana"
    assert K.split_text("sakura", "romaji")[:3] == ["sa", "ku", "ra"]


def test_distribute_and_build():
    d = K.distribute(["a", "b", "c"], 100)
    assert sum(d) == 100
    t = K.build(list(zip(["a", "b"], [30, 70])), "kf", "{\\an8}")
    assert t == "{\\an8}{\\kf30}a{\\kf70}b"
    assert K.to_syllables(t) == [("a", 30), ("b", 70)]
    assert K.strip_karaoke(t) == "{\\an8}ab"


def test_tap():
    assert K.tap_durations([1200, 1500], 1000, 2000) == [20, 30]
