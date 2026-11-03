import os, subprocess, shutil
import numpy as np
import pytest
from kaede.media import probe

pytestmark = pytest.mark.skipif(not probe.have_ffmpeg(), reason="ffmpeg missing")


@pytest.fixture(scope="module")
def clip(tmp_path_factory):
    p = str(tmp_path_factory.mktemp("m") / "t.mkv")
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=24:duration=4",
                    "-f", "lavfi", "-i", "sine=frequency=440:duration=4", "-g", "24", "-shortest", "-y", p], check=True)
    return p


def test_probe_keyframes_audio(clip):
    mi = probe.probe(clip)
    assert (mi.width, mi.height) == (320, 180) and abs(mi.fps - 24) < 0.01 and mi.has_audio
    kf = probe.keyframes(clip)
    assert kf[0] < 50 and 900 < kf[1] < 1100
    s = probe.decode_audio(clip, 8000)
    assert 3.9 < len(s) / 8000 < 4.1
    pk = probe.peaks(s, 8000, 100)
    assert pk.shape[1] == 2 and pk[:, 1].max() > 0.1
    segs = probe.detect_speech(s, 8000)
    assert segs and segs[0][0] < 200


def test_assrender_frame():
    from kaede.media.assrender import AssRenderer, available
    if not available(): pytest.skip("no libass")
    from kaede.core.ass import Document
    d = Document.parse(open(os.path.join(os.path.dirname(__file__), "..", "samples", "demo.ass"), encoding="utf-8").read())
    r = AssRenderer(640, 360); r.set_script(d.dump())
    assert (r.render(1500)[..., 3] > 0).sum() > 500
    assert (r.render(500)[..., 3] > 0).sum() == 0
