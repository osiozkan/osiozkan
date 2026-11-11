//! Makine çevirisi bağlayıcıları (kullanıcının kendi API anahtarıyla): DeepL, Google Cloud
//! Translation v2, LibreTranslate ve Claude. Altyazı etiketleri (`{...}`) ve `\N` korunur:
//! DeepL/Google/Libre'ye yalnızca görünür metin gider, etiketler başa geri eklenir; Claude'a
//! etiketleri koruması söylenir. İstek/yanıt biçimleri saf fonksiyonlardır (çevrimdışı testli).
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    DeepL,
    Google,
    Libre,
    Claude,
}

impl Provider {
    pub fn from_id(s: &str) -> Option<Provider> {
        match s {
            "deepl" => Some(Provider::DeepL),
            "google" => Some(Provider::Google),
            "libre" => Some(Provider::Libre),
            "claude" => Some(Provider::Claude),
            _ => None,
        }
    }
    pub const ALL: &'static [(&'static str, &'static str)] = &[("deepl", "DeepL"), ("google", "Google Cloud Translation"), ("libre", "LibreTranslate"), ("claude", "Claude (Anthropic)")];
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
    /// Claude için ek bağlam (ör. terim sözlüğü, dizi adı)
    pub context: String,
}

pub const CLAUDE_MODEL: &str = "claude-opus-5-5";

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
        Provider::Claude => {
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
    if cfg.key.trim().is_empty() && cfg.provider != Provider::Libre {
        return Err("API anahtarı girilmemiş (Araçlar → Makine çevirisi ayarları)".into());
    }
    let (url, headers, body) = build_request(cfg, texts);
    let mut req = ureq::post(&url).timeout(std::time::Duration::from_secs(120)).set("Content-Type", "application/json");
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
        MtConfig { provider: p, key: "abc:fx".into(), url: "http://localhost:5000/".into(), target: "tr".into(), source: String::new(), context: "Sözlük: senpai = senpai".into() }
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
}
