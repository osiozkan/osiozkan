//! Kayıt yedekleri (Aegisub "autoback"): bir dosyanın üzerine yazılmadan önceki hali
//! `<klasör>/<ad>.<yol özeti>.<YYYYMMDD-HHMMSS>.<uzantı>` olarak saklanır; dosya başına en yeni
//! `keep` yedek tutulur. Aynı adlı ama farklı klasördeki dosyalar yol özetiyle ayrılır.
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn path_hash(p: &Path) -> String {
    // FNV-1a 32 bit: kararlı (Rust sürümünden bağımsız) kısa özet
    let mut h: u32 = 0x811c9dc5;
    for b in p.to_string_lossy().as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{h:08x}")
}

/// UTC zaman damgası `YYYYMMDD-HHMMSS` (sıralanabilir).
pub fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Howard Hinnant'ın civil_from_days algoritması
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn prefix(src: &Path) -> (String, String) {
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "adsiz".into());
    let ext = src.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "ass".into());
    // aynı dosya göreli/mutlak yolla açılsa da (ya da iki klasördeki "tr.ass") doğru eşleşsin
    let canon = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    (format!("{stem}.{}.", path_hash(&canon)), ext)
}

/// `src`'nin şimdiki halini yedekler (dosya yoksa `None`) ve eskileri budar.
pub fn make(dir: &Path, src: &Path, keep: usize, now: SystemTime) -> Result<Option<PathBuf>, String> {
    if !src.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (pre, ext) = prefix(src);
    let mut dst = dir.join(format!("{pre}{}.{ext}", stamp(now)));
    // aynı saniyede ikinci kayıt: sonek ekle
    let mut k = 2;
    while dst.exists() {
        dst = dir.join(format!("{pre}{}-{k}.{ext}", stamp(now)));
        k += 1;
    }
    std::fs::copy(src, &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    for old in list(dir, src).into_iter().skip(keep.max(1)) {
        let _ = std::fs::remove_file(old.0);
    }
    Ok(Some(dst))
}

/// `src`'nin yedekleri, en yeni önce: (yol, zaman damgası metni).
pub fn list(dir: &Path, src: &Path) -> Vec<(PathBuf, String)> {
    let (pre, _) = prefix(src);
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<(PathBuf, String)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix(&pre)?;
            let st = rest.rsplit_once('.').map(|x| x.0).unwrap_or(rest).to_string();
            // yalnızca "YYYYMMDD-HHMMSS[-k]" (yedeğin yedeği vb. başka adlar karışmasın)
            let b = st.as_bytes();
            let ok = b.len() >= 15 && b[..8].iter().all(u8::is_ascii_digit) && b[8] == b'-' && b[9..15].iter().all(u8::is_ascii_digit)
                && (b.len() == 15 || (b[15] == b'-' && b.len() > 16 && b[16..].iter().all(u8::is_ascii_digit)));
            ok.then(|| (e.path(), st))
        })
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn damga() {
        assert_eq!(stamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(1_795_651_200 + 3723)), "20261126-010203");
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229-000000");
    }

    #[test]
    fn yedekle_listele_buda() {
        let base = std::env::temp_dir().join(format!("osisub-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("x/bolum.ass"), base.join("y/bolum.ass"));
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        let dir = base.join("autoback");
        assert!(make(&dir, &a, 3, UNIX_EPOCH).unwrap().is_none(), "olmayan dosya yedeklenmez");
        std::fs::write(&b, "B").unwrap();
        make(&dir, &b, 3, UNIX_EPOCH).unwrap();
        for k in 0..5u64 {
            std::fs::write(&a, format!("sürüm {k}")).unwrap();
            make(&dir, &a, 3, UNIX_EPOCH + Duration::from_secs(100 + k)).unwrap();
        }
        // aynı saniye
        make(&dir, &a, 3, UNIX_EPOCH + Duration::from_secs(104)).unwrap();
        let l = list(&dir, &a);
        assert_eq!(l.len(), 3);
        assert_eq!(std::fs::read_to_string(&l[0].0).unwrap(), "sürüm 4");
        assert!(l[0].1.starts_with("19700101-000144"));
        assert_eq!(list(&dir, &b).len(), 1, "farklı klasördeki aynı ad ayrı");
        // yedeğin kendisi açılıp kaydedilirse onun yedekleri asıl dosyanın listesine karışmaz
        let bk = l[0].0.clone();
        make(&dir, &bk, 3, UNIX_EPOCH + Duration::from_secs(500)).unwrap();
        assert_eq!(list(&dir, &a).len(), 3);
        let _ = std::fs::remove_dir_all(&base);
    }
}
