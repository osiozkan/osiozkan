use osisub_core::ass::{Dialogue, Script};
use osisub_core::qc::{check, Rules};
use std::time::Instant;

#[test]
#[ignore]
fn qc_100k() {
    let ds: Vec<Dialogue> = (0..100_000)
        .map(|i| Dialogue { start_time: i * 1500, end_time: i * 1500 + 1400, text: format!("{{\\i1}}Merhaba dünya satır {}{{\\i0}}\\Nikinci satır burada", i), ..Default::default() })
        .collect();
    let s = Script::from_dialogues(ds);
    let t = Instant::now();
    let issues = check(&s, &Rules::default());
    eprintln!("qc 100k: {:?}, sorun={}", t.elapsed(), issues.len());
    let t = Instant::now();
    let out = s.to_ass_string();
    eprintln!("yaz 100k: {:?}, {} bayt", t.elapsed(), out.len());
    let t = Instant::now();
    let p = Script::parse(&out);
    eprintln!("oku 100k: {:?}, {}", t.elapsed(), p.entries.len());
}
