use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CodeInfo { pub language: String }

#[derive(Debug, Serialize)]
pub struct Classification {
    pub primary_kind: String,
    pub kinds: Vec<String>,
    pub code: Option<CodeInfo>,
}

pub fn classify(text: &str) -> Classification {
    if text.is_empty() {
        return Classification { primary_kind: "text".into(), kinds: vec!["text".into()], code: None };
    }

    let mut kinds: Vec<String> = Vec::new();

    if is_full_url(text) || has_url(text) { kinds.push("url".into()); }
    if is_email(text) { kinds.push("email".into()); }
    if is_json(text) { kinds.push("json".into()); }
    if is_sql(text) { kinds.push("sql".into()); }
    if is_shell(text) { kinds.push("shell".into()); }

    let mut code = None;
    if let Some(lang) = detect_language(text) {
        kinds.push(format!("code:{}", lang));
        code = Some(CodeInfo { language: lang });
    }

    let primary_kind = pick_primary(&kinds, text);
    if kinds.is_empty() { kinds.push("text".into()); }

    Classification { primary_kind, kinds, code }
}

fn is_full_url(t: &str) -> bool {
    let t = t.trim();
    (t.starts_with("http://") || t.starts_with("https://")) && !t.contains(char::is_whitespace)
}
fn has_url(t: &str) -> bool { t.contains("http://") || t.contains("https://") }
fn is_email(t: &str) -> bool {
    let t = t.trim();
    t.contains('@') && t.matches('@').count() == 1 && {
        let (l, r) = t.split_once('@').unwrap();
        !l.is_empty() && r.contains('.') && !r.contains(char::is_whitespace) && !l.contains(char::is_whitespace)
    }
}
fn is_json(t: &str) -> bool {
    let t = t.trim();
    if !(t.starts_with('{') || t.starts_with('[')) { return false; }
    serde_json::from_str::<serde_json::Value>(t).is_ok()
}
fn is_sql(t: &str) -> bool {
    let head = t.trim_start().to_uppercase();
    head.starts_with("SELECT ") || head.starts_with("INSERT ") || head.starts_with("UPDATE ")
        || head.starts_with("DELETE ") || head.starts_with("CREATE ") || head.starts_with("ALTER ")
        || head.starts_with("DROP ") || head.starts_with("WITH ")
}
fn is_shell(t: &str) -> bool {
    let head = t.trim_start();
    if head.starts_with('$') { return true; }
    let first = head.split_whitespace().next().unwrap_or("");
    matches!(first, "sudo"|"cd"|"ls"|"git"|"npm"|"pnpm"|"yarn"|"brew"|"docker"|"kubectl"
        |"curl"|"wget"|"cargo"|"go"|"python"|"pip"|"node"|"make"|"bash"|"zsh"|"sh")
}

fn pick_primary(kinds: &[String], _text: &str) -> String {
    for k in kinds { if k == "json" { return "json".into(); } }
    for k in kinds { if k == "sql" { return "sql".into(); } }
    for k in kinds { if k == "shell" { return "shell".into(); } }
    for k in kinds { if k == "email" { return "email".into(); } }
    for k in kinds { if k.starts_with("code:") { return k.clone(); } }
    for k in kinds { if k == "url" { return "url".into(); } }
    "text".into()
}

fn detect_language(text: &str) -> Option<String> {
    if text.starts_with("#!") {
        let first_line = text.lines().next()?;
        if first_line.contains("python") { return Some("python".into()); }
        if first_line.contains("bash") || first_line.contains("zsh") || first_line.contains("/sh") { return None; }
        if first_line.contains("node") { return Some("javascript".into()); }
        if first_line.contains("ruby") { return Some("ruby".into()); }
    }
    let lower = text.to_ascii_lowercase();
    let scores = [
        ("python", count(&lower, &["def ", "import ", "self.", "elif ", "from ", "lambda ", "return "])),
        ("javascript", count(&lower, &["function ", "const ", "let ", "=>", "console.log", "require(", "import {"])),
        ("typescript", count(&lower, &[": string", ": number", "interface ", "type ", "enum "])),
        ("rust", count(&lower, &["fn ", "let mut", "impl ", "pub fn", "match ", "::<"])),
        ("go", count(&lower, &["package ", "func ", ":= ", "import (", "interface {"])),
        ("java", count(&lower, &["public class", "private ", "protected ", "static void"])),
        ("html", count(&lower, &["<html", "<div", "<span", "</p>", "<!doctype"])),
        ("css", count(&lower, &["{ ", "} ", "padding:", "margin:", "color:", "display:"])),
    ];
    let (lang, top) = scores.iter().max_by_key(|(_, n)| *n)?;
    if *top >= 2 { Some((*lang).into()) } else { None }
}

fn count(text: &str, needles: &[&str]) -> usize {
    needles.iter().map(|n| text.matches(n).count()).sum()
}
