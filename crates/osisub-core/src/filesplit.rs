//! Dosyayı parçalara böl (Subtitle Edit "Split"): kesme zamanlarına ya da eşit satır sayısına göre.
//! Her parça başlığı, stilleri, ekleri ve diğer bölümleri aynen taşır; satırlar başlangıç zamanına
//! göre dağıtılır. `rebase` ise her parçanın zamanı kendi kesme noktasından başlar (ör. bölümden
//! OP'yi ayırıp ayrı dosya yapmak). Kullanılmayan Extradata her parçadan temizlenir.
use crate::ass::{Entry, Script};

/// Kesme zamanlarına (ms, sırasız olabilir) göre `cuts.len() + 1` parça.
pub fn split_times(script: &Script, cuts: &[i64], rebase: bool) -> Vec<Script> {
    let mut c: Vec<i64> = cuts.iter().copied().filter(|&t| t > 0).collect();
    c.sort_unstable();
    c.dedup();
    let bounds: Vec<i64> = std::iter::once(0).chain(c.iter().copied()).collect();
    let mut parts: Vec<Script> = (0..bounds.len()).map(|_| { let mut s = script.clone(); s.entries.clear(); s }).collect();
    for e in &script.entries {
        match e {
            Entry::Dialogue(d) => {
                let k = bounds.partition_point(|&b| b <= d.start_time).saturating_sub(1);
                let mut d = d.clone();
                if rebase {
                    d.start_time -= bounds[k];
                    d.end_time -= bounds[k];
                }
                parts[k].entries.push(Entry::Dialogue(d));
            }
            other => {
                for p in parts.iter_mut() {
                    p.entries.push(other.clone());
                }
            }
        }
    }
    for p in parts.iter_mut() {
        p.clean_extradata();
    }
    parts
}

/// `n` parçaya eşit satır sayısıyla bölmek için kesme zamanları (yorumlar dahil, başlangıca göre).
pub fn equal_cuts(script: &Script, n: usize) -> Vec<i64> {
    let mut starts: Vec<i64> = script.dialogues().map(|d| d.start_time).collect();
    starts.sort_unstable();
    if n < 2 || starts.len() < n {
        return vec![];
    }
    (1..n).map(|k| starts[k * starts.len() / n]).collect()
}

/// Parça dosya adları: `ad_1.ass`, `ad_2.ass`…
pub fn part_paths(path: &std::path::Path, n: usize) -> Vec<std::path::PathBuf> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "parca".into());
    let ext = path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "ass".into());
    (1..=n).map(|k| path.with_file_name(format!("{stem}_{k}.{ext}"))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Dialogue;

    fn sc(starts: &[i64]) -> Script {
        let mut s = Script::new_default();
        s.entries.retain(|e| !matches!(e, Entry::Dialogue(_)));
        for &a in starts {
            s.entries.push(Entry::Dialogue(Dialogue { start_time: a, end_time: a + 1000, text: format!("{a}"), ..Default::default() }));
        }
        s
    }
    fn starts(s: &Script) -> Vec<i64> {
        s.dialogues().map(|d| d.start_time).collect()
    }

    #[test]
    fn zamana_gore() {
        let s = sc(&[0, 5000, 90000, 95000, 200000]);
        let p = split_times(&s, &[90000, 0, 180000], false);
        assert_eq!(p.len(), 3);
        assert_eq!(starts(&p[0]), [0, 5000]);
        assert_eq!(starts(&p[1]), [90000, 95000]);
        assert_eq!(starts(&p[2]), [200000]);
        // başlık ve stiller her parçada
        assert!(p.iter().all(|x| x.styles().count() == s.styles().count() && x.info("ScriptType").is_some()));
        let r = split_times(&s, &[90000], true);
        assert_eq!(starts(&r[1]), [0, 5000, 110000]);
        assert_eq!(r[1].dialogues().next().unwrap().end_time, 1000);
    }

    #[test]
    fn esit_ve_adlar() {
        let s = sc(&[0, 1000, 2000, 3000, 4000, 5000]);
        let c = equal_cuts(&s, 3);
        assert_eq!(c, [2000, 4000]);
        let p = split_times(&s, &c, false);
        assert_eq!(p.iter().map(|x| x.dialogues().count()).collect::<Vec<_>>(), [2, 2, 2]);
        assert!(equal_cuts(&s, 1).is_empty());
        let n = part_paths(std::path::Path::new("/a/bolum 01.ass"), 2);
        assert_eq!(n[1], std::path::PathBuf::from("/a/bolum 01_2.ass"));
    }
}
