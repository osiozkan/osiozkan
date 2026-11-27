//! Makine çevirisi bağlayıcıları (kullanıcının kendi API anahtarıyla): DeepL, Google Cloud
//! Translation v2, LibreTranslate, Claude ve OpenAI uyumlu sohbet sunucuları (Ollama, LM Studio,
//! llama.cpp, OpenRouter… — yerel olanlar anahtarsız ve ücretsiz). Altyazı etiketleri (`{...}`) ve `\N` korunur:
//! DeepL/Google/Libre'ye yalnızca görünür metin gider, etiketler başa geri eklenir; Claude'a
//! etiketleri koruması söylenir. İstek/yanıt biçimleri saf fonksiyonlardır (çevrimdışı testli).
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    DeepL,
    Google,
    Libre,
    Claude,
    /// OpenAI uyumlu `/chat/completions` (url = taban adres, ör. `http://localhost:11434/v1`)
    OpenAi,
}

impl Provider {
    pub fn from_id(s: &str) -> Option<Provider> {
        match s {
            "deepl" => Some(Provider::DeepL),
            "google" => Some(Provider::Google),
            "libre" => Some(Provider::Libre),
            "claude" => Some(Provider::Claude),
            "openai" => Some(Provider::OpenAi),
            _ => None,
        }
    }
    pub const ALL: &'static [(&'static str, &'static str)] = &[("deepl", "DeepL"), ("google", "Google Cloud Translation"), ("libre", "LibreTranslate"), ("claude", "Claude (Anthropic)"), ("openai", "Yerel YZ / OpenAI uyumlu (Ollama, LM Studio…)")];
}

#[derive(Clone, Debug)]
pub struct MtConfig {
    pub provider: Provider,
    pub key: String,
    /// LibreTranslate sunucu adresi
    pub url: String,
    /// hedef dil (ISO 639-1, ör. "tr")
    pub target: String,
    /// kaynak dil ("" = otomatik)
    pub source: String,
    /// Claude/YZ için ek bağlam (ör. terim sözlüğü, dizi adı)
    pub context: String,
    /// OpenAI uyumlu sunucuda model adı (ör. "qwen2.5:14b")
    pub model: String,
}

pub const CLAUDE_MODEL: &str = "claude-opus-5-5";

/// Claude ve OpenAI uyumlu YZ için ortak sistem istemi.
fn llm_system(cfg: &MtConfig) -> String {
    let mut system = format!(
                "You are a professional anime subtitle translator. Translate each subtitle line into the language with ISO code '{}'{}. \
                 Keep ASS override tags in curly braces (e.g. {{\\i1}}) and line breaks written as \\N exactly where they belong. \
                 Write natural, concise subtitle language that fits on screen; keep honorifics and names as they are. \
                 Return exactly one translation per input line, in the same order.",
        cfg.target,
        if cfg.source.is_empty() { String::new() } else { format!(" from '{}'", cfg.source) }
    );
    if !cfg.context.trim().is_empty() {
        system.push_str("\n\nContext and glossary from the user:\n");
        system.push_str(cfg.context.trim());
    }
    system
}

/// YZ yanıtındaki JSON'u bulur (``` çitleri, öncesinde/sonrasında açıklama olabilir).
fn extract_json(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) { return Some(v); }
    let (a, b) = (text.find('{')?, text.rfind('}')?);
    (a < b).then(|| serde_json::from_str(&text[a..=b]).ok()).flatten()
}

/// Satırı (başlangıç etiketleri, metin) diye ayırır; metindeki `\N` → "\n".
pub fn split_leading_tags(line: &str) -> (String, String) {
    let mut lead = String::new();
    let mut rest = line;
    while rest.starts_with('{') {
        match rest.find('}') {
            Some(e) => {
                lead.push_str(&rest[..=e]);
                rest = &rest[e + 1..];
            }
            None => break,
        }
    }
    (lead, rest.replace("\\N", "\n"))
}

fn rejoin(lead: &str, text: &str) -> String {
    format!("{}{}", lead, text.trim().replace("\r\n", "\n").replace('\n', "\\N"))
}

/// Kopyala-yapıştır çeviri (Subtitle Edit Copy-paste translate): satırların görünür metinlerini, aralarında
/// yalnız `sep` içeren satırla, en çok `max` karakterlik bloklara paketler. (başlangıç, bitiş, blok metni).
pub fn cp_blocks(texts: &[String], sep: &str, max: usize) -> Vec<(usize, usize, String)> {
    let mut out = vec![];
    let (mut start, mut cur) = (0, String::new());
    let join = format!("\n{}\n", sep);
    for (i, t) in texts.iter().enumerate() {
        let body = split_leading_tags(t).1.trim().replace("\n\n", "\n");
        if !cur.is_empty() && cur.len() + join.len() + body.len() > max {
            out.push((start, i, std::mem::take(&mut cur)));
            start = i;
        }
        if i > start { cur.push_str(&join); }
        cur.push_str(&body);
    }
    if start < texts.len() {
        out.push((start, texts.len(), cur));
    }
    out
}

/// Yapıştırılan çeviriyi `sep` satırlarından böler; parça sayısı kaynak satır sayısına eşit olmalı.
/// Kaynak satırların baştaki etiketleri geri eklenir, satır içi satır sonları `\N` olur.
pub fn cp_unpack(srcs: &[String], pasted: &str, sep: &str) -> Result<Vec<String>, String> {
    let mut parts: Vec<String> = vec![String::new()];
    for line in pasted.replace("\r\n", "\n").lines() {
        if line.trim() == sep.trim() {
            parts.push(String::new());
        } else {
            let p = parts.last_mut().unwrap();
            if !p.is_empty() { p.push('\n'); }
            p.push_str(line.trim_end());
        }
    }
    // baş/son boş parçalar (çevirmenin eklediği boş satırlar) yok sayılır
    while parts.len() > srcs.len() && parts.last().is_some_and(|p| p.trim().is_empty()) { parts.pop(); }
    while parts.len() > srcs.len() && parts.first().is_some_and(|p| p.trim().is_empty()) { parts.remove(0); }
    if parts.len() != srcs.len() {
        return Err(format!("{} satır bekleniyordu, {} parça geldi (ayraç \"{}\" satırları kaybolmuş olabilir)", srcs.len(), parts.len(), sep.trim()));
    }
    Ok(srcs.iter().zip(parts).map(|(s, t)| rejoin(&split_leading_tags(s).0, t.trim())).collect())
}

/// (adres, başlıklar, gövde) — ağ katmanından bağımsız.
pub fn build_request(cfg: &MtConfig, texts: &[String]) -> (String, Vec<(String, String)>, Value) {
    let plain: Vec<String> = texts.iter().map(|t| split_leading_tags(t).1).collect();
    match cfg.provider {
        Provider::DeepL => {
            let host = if cfg.key.trim().ends_with(":fx") { "api-free.deepl.com" } else { "api.deepl.com" };
            let mut body = json!({ "text": plain, "target_lang": cfg.target.to_uppercase() });
            if !cfg.source.is_empty() {
                body["source_lang"] = json!(cfg.source.to_uppercase());
            }
            (format!("https://{host}/v2/translate"), vec![("Authorization".into(), format!("DeepL-Auth-Key {}", cfg.key.trim()))], body)
        }
        Provider::Google => {
            let mut body = json!({ "q": plain, "target": cfg.target, "format": "text" });
            if !cfg.source.is_empty() {
                body["source"] = json!(cfg.source);
            }
            (format!("https://translation.googleapis.com/language/translate/v2?key={}", cfg.key.trim()), vec![], body)
        }
        Provider::Libre => {
            let mut body = json!({ "q": plain, "source": if cfg.source.is_empty() { "auto" } else { cfg.source.as_str() }, "target": cfg.target, "format": "text" });
            if !cfg.key.trim().is_empty() {
                body["api_key"] = json!(cfg.key.trim());
            }
            (format!("{}/translate", cfg.url.trim_end_matches('/')), vec![], body)
        }
        Provider::OpenAi => {
            let body = json!({
                "model": cfg.model.trim(),
                "temperature": 0.2,
                "stream": false,
                "messages": [
                    { "role": "system", "content": llm_system(cfg) + "\n\nAnswer ONLY with a JSON object of the form {\"translations\": [\"...\", ...]} and nothing else." },
                    { "role": "user", "content": serde_json::to_string(&json!({ "lines": texts })).unwrap_or_default() }
                ]
            });
            let headers = if cfg.key.trim().is_empty() { vec![] } else { vec![("Authorization".into(), format!("Bearer {}", cfg.key.trim()))] };
            (format!("{}/chat/completions", cfg.url.trim().trim_end_matches('/')), headers, body)
        }
        Provider::Claude => {
            let system = llm_system(cfg);
            let schema = json!({
                "type": "object",
                "properties": { "translations": { "type": "array", "items": { "type": "string" } } },
                "required": ["translations"],
                "additionalProperties": false
            });
            let body = json!({
                "model": CLAUDE_MODEL,
                "max_tokens": 16000,
                "system": system,
                "fallbacks": "default",
                "output_config": { "effort": "low", "format": { "type": "json_schema", "schema": schema } },
                "messages": [{ "role": "user", "content": serde_json::to_string(&json!({ "lines": texts })).unwrap_or_default() }]
            });
            (
                "https://api.anthropic.com/v1/messages".into(),
                vec![
                    ("x-api-key".into(), cfg.key.trim().to_string()),
                    ("anthropic-version".into(), "2026-10-21".into()),
                    ("anthropic-beta".into(), "server-side-fallback-2026-07-01".into()),
                ],
                body,
            )
        }
    }
}

/// Yanıt gövdesini çevirilere dönüştürür; baştaki etiketler (Claude dışı) geri eklenir.
pub fn parse_response(cfg: &MtConfig, texts: &[String], body: &Value) -> Result<Vec<String>, String> {
    let raw: Vec<String> = match cfg.provider {
        Provider::DeepL => body["translations"].as_array().ok_or("DeepL: beklenmeyen yanıt")?.iter().map(|t| t["text"].as_str().unwrap_or("").to_string()).collect(),
        Provider::Google => body["data"]["translations"].as_array().ok_or("Google: beklenmeyen yanıt")?.iter().map(|t| t["translatedText"].as_str().unwrap_or("").to_string()).collect(),
        Provider::Libre => match &body["translatedText"] {
            Value::Array(a) => a.iter().map(|t| t.as_str().unwrap_or("").to_string()).collect(),
            Value::String(s) => vec![s.clone()],
            _ => return Err(format!("LibreTranslate: {}", body["error"].as_str().unwrap_or("beklenmeyen yanıt"))),
        },
        Provider::OpenAi => {
            let text = body["choices"][0]["message"]["content"].as_str().ok_or_else(|| format!("YZ: beklenmeyen yanıt ({})", body["error"]["message"].as_str().or(body["error"].as_str()).unwrap_or("choices yok")))?;
            let v = extract_json(text).ok_or("YZ: yanıtta JSON bulunamadı (daha büyük/talimat izleyen bir model deneyin)")?;
            let arr = v["translations"].as_array().or(v.as_array()).ok_or("YZ: translations yok")?;
            let out: Vec<String> = arr.iter().map(|t| t.as_str().unwrap_or("").to_string()).collect();
            if out.len() != texts.len() {
                return Err(format!("YZ: {} satır istendi, {} geldi (daha az satır seçin)", texts.len(), out.len()));
            }
            return Ok(out.into_iter().map(|t| t.replace('\n', "\\N")).collect());
        }
        Provider::Claude => {
            if body["stop_reason"].as_str() == Some("refusal") {
                return Err("Claude isteği reddetti (güvenlik sınıflandırıcısı)".into());
            }
            if body["stop_reason"].as_str() == Some("max_tokens") {
                return Err("Claude yanıtı kesildi (çok uzun parti; daha az satır seçin)".into());
            }
            let text: String = body["content"].as_array().ok_or("Claude: beklenmeyen yanıt")?.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            let v: Value = serde_json::from_str(&text).map_err(|e| format!("Claude: JSON okunamadı: {e}"))?;
            let out: Vec<String> = v["translations"].as_array().ok_or("Claude: translations yok")?.iter().map(|t| t.as_str().unwrap_or("").to_string()).collect();
            if out.len() != texts.len() {
                return Err(format!("Claude: {} satır istendi, {} geldi", texts.len(), out.len()));
            }
            return Ok(out.into_iter().map(|t| t.replace('\n', "\\N")).collect());
        }
    };
    if raw.len() != texts.len() {
        return Err(format!("{} satır istendi, {} geldi", texts.len(), raw.len()));
    }
    Ok(texts.iter().zip(raw).map(|(src, tr)| rejoin(&split_leading_tags(src).0, &tr)).collect())
}

/// Ağ çağrısı (engelleyici; arka plan iş parçacığında çağırın). Hata iletisi API'nin kendi iletisini içerir.
pub fn translate(cfg: &MtConfig, texts: &[String]) -> Result<Vec<String>, String> {
    if texts.is_empty() {
        return Ok(vec![]);
    }
    if cfg.provider == Provider::OpenAi && cfg.model.trim().is_empty() {
        return Err("Model adı girilmemiş (ör. qwen2.5:14b; Ollama'da `ollama list`)".into());
    }
    if cfg.key.trim().is_empty() && cfg.provider != Provider::Libre && cfg.provider != Provider::OpenAi {
        return Err("API anahtarı girilmemiş (Araçlar → Makine çevirisi ayarları)".into());
    }
    let (url, headers, body) = build_request(cfg, texts);
    let secs = if cfg.provider == Provider::OpenAi { 600 } else { 120 }; // yerel modeller yavaş
    let mut req = ureq::post(&url).timeout(std::time::Duration::from_secs(secs)).set("Content-Type", "application/json");
    for (k, v) in &headers {
        req = req.set(k, v);
    }
    let resp = match req.send_string(&body.to_string()) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let t = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&t).ok().and_then(|v| v["error"]["message"].as_str().or(v["message"].as_str()).or(v["error"].as_str()).map(String::from)).unwrap_or(t);
            return Err(format!("HTTP {code}: {msg}"));
        }
        Err(e) => return Err(format!("bağlantı hatası: {e}")),
    };
    let v: Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    parse_response(cfg, texts, &v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(p: Provider) -> MtConfig {
        MtConfig { provider: p, key: "abc:fx".into(), url: "http://localhost:5000/".into(), target: "tr".into(), source: String::new(), context: "Sözlük: senpai = senpai".into(), model: "qwen2.5:14b".into() }
    }

    #[test]
    fn tags_are_kept_outside_mt() {
        let texts = vec!["{\\an8\\i1}Hello\\Nworld".to_string(), "Plain".to_string()];
        let (url, h, body) = build_request(&cfg(Provider::DeepL), &texts);
        assert_eq!(url, "https://api-free.deepl.com/v2/translate");
        assert_eq!(h[0].1, "DeepL-Auth-Key abc:fx");
        assert_eq!(body["text"][0], "Hello\nworld");
        assert_eq!(body["target_lang"], "TR");
        let out = parse_response(&cfg(Provider::DeepL), &texts, &json!({"translations": [{"text": "Merhaba\ndünya"}, {"text": "Düz"}]})).unwrap();
        assert_eq!(out, vec!["{\\an8\\i1}Merhaba\\Ndünya", "Düz"]);
        let g = parse_response(&cfg(Provider::Google), &texts[1..], &json!({"data": {"translations": [{"translatedText": "Düz"}]}})).unwrap();
        assert_eq!(g, vec!["Düz"]);
        let (lurl, _, lb) = build_request(&cfg(Provider::Libre), &texts);
        assert_eq!((lurl.as_str(), lb["source"].as_str()), ("http://localhost:5000/translate", Some("auto")));
        assert!(parse_response(&cfg(Provider::DeepL), &texts, &json!({"translations": []})).is_err());
    }

    #[test]
    fn claude_request_and_response() {
        let texts = vec!["{\\i1}Senpai!".to_string(), "Wait\\Nfor me".to_string()];
        let (url, h, body) = build_request(&cfg(Provider::Claude), &texts);
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
        assert!(h.iter().any(|(k, v)| k == "anthropic-beta" && v == "server-side-fallback-2026-07-01"));
        assert_eq!(body["model"], CLAUDE_MODEL);
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert!(body.get("temperature").is_none() && body.get("thinking").is_none());
        assert!(body["system"].as_str().unwrap().contains("senpai"));
        let resp = json!({"stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "{\"translations\": [\"{\\\\i1}Senpai!\", \"Bekle\\nbeni\"]}"}]});
        assert_eq!(parse_response(&cfg(Provider::Claude), &texts, &resp).unwrap(), vec!["{\\i1}Senpai!", "Bekle\\Nbeni"]);
        assert!(parse_response(&cfg(Provider::Claude), &texts, &json!({"stop_reason": "refusal", "content": []})).unwrap_err().contains("reddetti"));
    }

    #[test]
    fn openai_compatible_request_and_response() {
        let texts = vec!["{\\i1}Senpai!".to_string(), "Wait\\Nfor me".to_string()];
        let mut c = cfg(Provider::OpenAi);
        c.url = "http://localhost:11434/v1/".into();
        c.key = String::new();
        let (url, h, body) = build_request(&c, &texts);
        assert_eq!(url, "http://localhost:11434/v1/chat/completions");
        assert!(h.is_empty());
        assert_eq!(body["model"], "qwen2.5:14b");
        assert!(body["messages"][0]["content"].as_str().unwrap().contains("senpai"));
        // çitli ve açıklamalı yanıt da okunur
        let resp = json!({"choices": [{"message": {"content": "Sure!\n```json\n{\"translations\": [\"{\\\\i1}Senpai!\", \"Bekle\\nbeni\"]}\n```"}}]});
        assert_eq!(parse_response(&c, &texts, &resp).unwrap(), vec!["{\\i1}Senpai!", "Bekle\\Nbeni"]);
        assert!(parse_response(&c, &texts, &json!({"choices": [{"message": {"content": "{\"translations\": [\"x\"]}"}}]})).unwrap_err().contains("2 satır"));
        assert!(parse_response(&c, &texts, &json!({"error": {"message": "model not found"}})).unwrap_err().contains("model not found"));
        c.key = "sk-1".into();
        assert_eq!(build_request(&c, &texts).1[0].1, "Bearer sk-1");
    }

    #[test]
    fn openai_compatible_over_http() {
        // yerel sahte sunucu: isteği okur, sabit yanıt döner (Ollama /v1/chat/completions gibi)
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = vec![0u8; 65536];
            let mut got = Vec::new();
            loop {
                let n = c.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&got).to_string();
                if let Some(he) = t.find("\r\n\r\n") {
                    let len: usize = t.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
                    if got.len() >= he + 4 + len { break; }
                }
                if n == 0 { break; }
            }
            let body = r#"{"choices":[{"message":{"role":"assistant","content":"{\"translations\":[\"Merhaba\"]}"}}]}"#;
            write!(c, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        let mut c = cfg(Provider::OpenAi);
        c.key = String::new();
        c.url = format!("http://127.0.0.1:{port}/v1");
        assert_eq!(translate(&c, &["Hello".to_string()]).unwrap(), vec!["Merhaba"]);
        let req = h.join().unwrap();
        assert!(req.starts_with("POST /v1/chat/completions"));
        assert!(!req.to_ascii_lowercase().contains("authorization"));
        c.model = " ".into();
        assert!(translate(&c, &["x".to_string()]).unwrap_err().contains("Model"));
    }

    #[test]
    fn copy_paste_blocks_roundtrip() {
        let texts: Vec<String> = vec!["{\\an8}Hello\\Nthere".into(), "Second".into(), "Third line".into()];
        let b = cp_blocks(&texts, ".", 5000);
        assert_eq!(b, vec![(0, 3, "Hello\nthere\n.\nSecond\n.\nThird line".to_string())]);
        // küçük sınırda bölünür, satır bölünmez
        let b2 = cp_blocks(&texts, ".", 19);
        assert_eq!(b2.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), vec![(0, 1), (1, 3)]);
        let pasted = "\r\nMerhaba\r\noradaki\r\n.\r\nİkinci\r\n .\r\nÜçüncü satır\r\n\r\n";
        assert_eq!(cp_unpack(&texts, pasted, ".").unwrap(), vec!["{\\an8}Merhaba\\Noradaki", "İkinci", "Üçüncü satır"]);
        assert!(cp_unpack(&texts, "Merhaba. İkinci. Üçüncü", ".").unwrap_err().contains("3 satır"));
    }
}
