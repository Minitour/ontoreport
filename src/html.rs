//! Render a report as one self-contained HTML page (no external assets, light and dark themes).

use std::collections::BTreeMap;
use std::fmt::Write;

use serde_json::Value;

use crate::report::{Finding, Report, Section};

/// Findings shown per check in the page; the JSON report always has all of them.
const SHOWN_PER_CHECK: usize = 200;

const CSS: &str = r#"
:root{--bg:#F6F7F5;--paper:#FFF;--fg:#1B2420;--muted:#5B6862;--rule:#D7DDD9;--accent:#1F6F5C;--accent-soft:#E1F0EB;
--warn:#8A5A12;--warn-soft:#F7ECDA;--ok:#2E6B3F;--ok-soft:#E3F1E6;--no:#8B2E2E;--no-soft:#F6E3E1;
--sans:"Segoe UI",system-ui,-apple-system,Arial,sans-serif;--mono:Consolas,"SFMono-Regular",monospace}
@media (prefers-color-scheme:dark){:root{--bg:#121614;--paper:#1A201D;--fg:#E4EAE6;--muted:#9BA8A1;--rule:#2E3833;
--accent:#6CC5AB;--accent-soft:#1E302A;--warn:#E3B66A;--warn-soft:#30281A;--ok:#8CCB9B;--ok-soft:#1E2D22;--no:#E79A93;--no-soft:#35201E}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 var(--sans)}
main{max-width:1040px;margin:0 auto;padding:28px 16px 60px}h1{font-size:1.7rem;margin:0 0 4px}
h2{font-size:1.25rem;margin:34px 0 8px;padding-top:10px;border-top:2px solid var(--rule)}h3{font-size:1rem;margin:18px 0 6px}
.muted{color:var(--muted)}code{font-family:var(--mono);font-size:.88em}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px;margin:10px 0}
.stat{background:var(--paper);border:1px solid var(--rule);border-radius:9px;padding:10px 12px}
.stat b{display:block;font-size:1.35rem;color:var(--accent)}.stat span{font-size:.85rem;color:var(--muted)}
.note{border-left:4px solid var(--accent);background:var(--accent-soft);border-radius:6px;padding:8px 12px;margin:8px 0;font-size:.92rem}
.wrap{overflow-x:auto}table{border-collapse:collapse;width:100%;background:var(--paper);font-size:.88rem}
th,td{border:1px solid var(--rule);padding:6px 8px;text-align:left;vertical-align:top}th{background:var(--accent-soft)}
.tag{display:inline-block;font-size:.75rem;font-weight:600;padding:1px 7px;border-radius:999px}
.ok{background:var(--ok-soft);color:var(--ok)}.no{background:var(--no-soft);color:var(--no)}.part{background:var(--warn-soft);color:var(--warn)}
details{margin:6px 0}summary{cursor:pointer;font-weight:600}
"#;

pub fn render(r: &Report) -> String {
    let mut h = String::with_capacity(1 << 20);
    let _ = write!(
        h,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" \
         content=\"width=device-width,initial-scale=1\"><title>ontoreport</title><style>{CSS}</style></head><body><main>"
    );
    let _ = write!(
        h,
        "<h1>Ontology quality report</h1><p class=\"muted\">{} {} · inputs: {}</p>",
        r.tool,
        r.version,
        esc(&r.inputs.join(", "))
    );

    let _ = write!(h, "<div class=\"grid\">");
    for s in &r.sections {
        let (tag, label) = if !s.ran {
            ("part", "not run".to_string())
        } else if s.findings.is_empty() {
            ("ok", "no findings".to_string())
        } else {
            ("no", format!("{} findings", s.findings.len()))
        };
        let _ = write!(
            h,
            "<div class=\"stat\"><span>{}</span><b><a href=\"#{}\" style=\"color:inherit;text-decoration:none\">{}</a></b>\
             <span class=\"tag {tag}\">{label}</span></div>",
            esc(s.title),
            s.id,
            esc(s.title)
        );
    }
    let _ = write!(h, "</div>");

    for s in &r.sections {
        section(&mut h, s);
    }

    if !r.input_checks.is_empty() {
        let _ = write!(h, "<h2>Checks carried by the inputs</h2><div class=\"wrap\"><table><tr><th>Check</th><th>Result</th><th>Summary</th></tr>");
        for c in &r.input_checks {
            let (cls, txt) = if c.ok {
                ("ok", "passed")
            } else {
                ("no", "failed")
            };
            let _ = write!(
                h,
                "<tr><td>{}</td><td><span class=\"tag {cls}\">{txt}</span></td><td>{}</td></tr>",
                esc(&c.name),
                esc(&c.summary)
            );
        }
        let _ = write!(h, "</table></div>");
    }
    if !r.recorded.is_empty() {
        let _ = write!(
            h,
            "<h2>Problems already recorded in the project</h2><details><summary>{} recorded</summary>\
             <div class=\"wrap\"><table><tr><th>Check</th><th>Element</th><th>Detail</th></tr>",
            r.recorded.len()
        );
        for f in &r.recorded {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&f.check),
                esc(&f.element),
                esc(&f.detail)
            );
        }
        let _ = write!(h, "</table></div></details>");
    }
    let _ = writeln!(h, "</main></body></html>");
    h
}

fn section(h: &mut String, s: &Section) {
    let _ = write!(h, "<h2 id=\"{}\">{}</h2>", s.id, esc(s.title));
    if !s.ran {
        for n in &s.notes {
            let _ = write!(h, "<div class=\"note\">Not run: {}</div>", esc(n));
        }
        return;
    }
    let _ = write!(h, "<div class=\"grid\">");
    for m in &s.metrics {
        let _ = write!(
            h,
            "<div class=\"stat\"><b>{}</b><span>{}</span></div>",
            esc(&value_text(&m.value)),
            esc(&m.name)
        );
    }
    let _ = write!(h, "</div>");
    for n in &s.notes {
        let _ = write!(h, "<div class=\"note\">{}</div>", esc(n));
    }
    let mut by_check: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in &s.findings {
        by_check.entry(f.check).or_default().push(f);
    }
    for (check, fs) in by_check {
        let _ = write!(
            h,
            "<details><summary>{} ({})</summary><div class=\"wrap\"><table>\
             <tr><th>Element</th><th>Where</th><th>Status</th><th>Detail</th></tr>",
            esc(check),
            fs.len()
        );
        for f in fs.iter().take(SHOWN_PER_CHECK) {
            let origin = serde_json::to_value(f.origin)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string));
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&f.element),
                esc(&match &f.source {
                    Some(src) =>
                        format!("{} · {src}", origin.unwrap_or_default().replace('_', " ")),
                    None => origin.unwrap_or_default().replace('_', " "),
                }),
                esc(f.status.as_deref().unwrap_or("")),
                esc(&f.detail)
            );
        }
        if fs.len() > SHOWN_PER_CHECK {
            let _ = write!(
                h,
                "<tr><td colspan=\"4\" class=\"muted\">{} more in the JSON report</td></tr>",
                fs.len() - SHOWN_PER_CHECK
            );
        }
        let _ = write!(h, "</table></div></details>");
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| format!("{k} {}", value_text(v)))
            .collect::<Vec<_>>()
            .join(" · "),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}
