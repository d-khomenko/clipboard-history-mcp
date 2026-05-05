use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::sync::Mutex;

#[derive(Debug)]
pub struct SecretHit {
    pub kind: String,
    pub value: String,
    pub last_chars: String,
}

#[derive(Debug, Deserialize)]
struct GitleaksConfig {
    rules: Option<Vec<RawRule>>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    regex: Option<String>,
}

struct CompiledRule { kind: String, re: Regex }

static RULES: Lazy<Mutex<Vec<CompiledRule>>> = Lazy::new(|| {
    let raw = include_str!("../../vendor/gitleaks.toml");
    let cfg: GitleaksConfig = toml::from_str(raw).unwrap_or(GitleaksConfig { rules: None });
    let mut out: Vec<CompiledRule> = cfg.rules.unwrap_or_default().into_iter()
        .filter_map(|r| {
            let pattern = r.regex.as_deref()?;
            let re = Regex::new(pattern).ok()?;
            Some(CompiledRule { kind: r.id, re })
        })
        .collect();
    out.push(CompiledRule {
        kind: "jwt".into(),
        re: Regex::new(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap(),
    });
    Mutex::new(out)
});

pub fn detect_secret(text: &str) -> Option<SecretHit> {
    if text.is_empty() { return None; }
    let rules = RULES.lock().ok()?;
    for rule in rules.iter() {
        if let Some(m) = rule.re.find(text) {
            let value = m.as_str().to_string();
            let last_chars = value.chars().rev().take(6).collect::<String>().chars().rev().collect();
            return Some(SecretHit { kind: rule.kind.clone(), value, last_chars });
        }
    }
    drop(rules);
    detect_credit_card(text)
}

fn detect_credit_card(text: &str) -> Option<SecretHit> {
    let cc_re = Regex::new(r"\b(?:\d[ -]?){13,19}\b").ok()?;
    let m = cc_re.find(text)?;
    let raw = m.as_str();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if !(13..=19).contains(&digits.len()) || !luhn(&digits) { return None; }
    let last_chars = digits.chars().rev().take(4).collect::<String>().chars().rev().collect();
    Some(SecretHit { kind: "credit_card".into(), value: digits, last_chars })
}

fn luhn(s: &str) -> bool {
    let mut sum = 0u32;
    let mut alt = false;
    for c in s.chars().rev() {
        let mut n = c.to_digit(10).unwrap_or(0);
        if alt { n *= 2; if n > 9 { n -= 9; } }
        sum += n;
        alt = !alt;
    }
    sum % 10 == 0
}
