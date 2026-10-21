use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_subforge"))
}
fn fx(n: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(n)
}
fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sf-cli-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

#[test]
fn convert_ass_to_srt_and_back() {
    let srt = tmp("a.srt");
    let st = bin().args(["convert"]).arg(fx("sample.ass")).arg(&srt).status().unwrap();
    assert!(st.success());
    let text = std::fs::read_to_string(&srt).unwrap();
    assert!(text.contains("00:00:01,000 --> 00:00:03,000"), "{}", text);
    let ass = tmp("b.ass");
    assert!(bin().arg("convert").arg(&srt).arg(&ass).status().unwrap().success());
    let out = bin().arg("info").arg(&ass).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("dialogue"));
}

#[test]
fn shift_in_place_makes_backup() {
    let f = tmp("shift.ass");
    std::fs::copy(fx("sample.ass"), &f).unwrap();
    assert!(bin().arg("shift").arg(&f).arg("500").status().unwrap().success());
    let now = std::fs::read_to_string(&f).unwrap();
    assert!(now.contains("0:00:01.50,0:00:03.50"), "{}", now);
    let mut bak = f.clone().into_os_string();
    bak.push(".bak");
    assert!(std::fs::read_to_string(PathBuf::from(bak)).unwrap().contains("0:00:01.00,0:00:03.00"));
}

#[test]
fn qa_exit_code_reflects_issues() {
    let ok = bin().arg("qa").arg(fx("sample.ass")).args(["--min-duration", "100", "--max-cps", "100"]).output().unwrap();
    // sample has an empty-ish/double space line -> may report; just make sure the command runs and prints a summary
    assert!(String::from_utf8_lossy(&ok.stdout).contains("issue(s)"));
    let bad = bin().arg("qa").arg(fx("typeset.ass")).output().unwrap();
    assert_eq!(bad.status.code(), Some(1), "long events must be reported with exit code 1");
}

#[test]
fn clean_resample_and_stats() {
    let out = tmp("c.ass");
    assert!(bin().arg("clean").arg(fx("sample.ass")).args(["--fix", "--case", "sentence", "--turkish", "-o"]).arg(&out).status().unwrap().success());
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(t.contains("Hello world"), "{}", t); // double space fixed
    let out2 = tmp("d.ass");
    assert!(bin().arg("resample").arg(&out).args(["2560", "1440", "-o"]).arg(&out2).status().unwrap().success());
    let t2 = std::fs::read_to_string(&out2).unwrap();
    assert!(t2.contains("PlayResX: 2560") && t2.contains("PlayResY: 1440"));
    let st = bin().arg("stats").arg(&out2).output().unwrap();
    assert!(String::from_utf8_lossy(&st.stdout).contains("lines 2"));
}

#[test]
fn builtin_macros_listed_and_run() {
    let l = bin().args(["macros", "builtin:strip-tags.lua"]).output().unwrap();
    assert!(String::from_utf8_lossy(&l.stdout).contains("Strip tags"));
    let out = tmp("e.ass");
    let st = bin().arg("run").arg(fx("typeset.ass")).arg("builtin:strip-tags.lua").arg("Strip tags").arg("-o").arg(&out).status().unwrap();
    assert!(st.success());
    assert!(!std::fs::read_to_string(&out).unwrap().contains("\\pos"));
}

#[test]
fn legacy_turkish_encoding_roundtrip() {
    let src = "[Script Info]\nTitle: t\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Şimdi çocuklar İstanbul'a ığ\n";
    let f = tmp("tr1254.ass");
    std::fs::write(&f, subforge::encoding::encode(src, "windows-1254", false)).unwrap();
    let out = tmp("tr.srt");
    assert!(bin().arg("convert").arg(&f).arg(&out).status().unwrap().success());
    assert!(std::fs::read_to_string(&out).unwrap().contains("Şimdi çocuklar İstanbul'a ığ"));
}
