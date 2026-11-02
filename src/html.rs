//! Render a report as one self-contained HTML page: a dashboard of KPIs, charts drawn as inline SVG,
//! drill-down hierarchies and searchable findings. No external assets; light and dark themes; works
//! without JavaScript (the small inline script only adds search and expand-all).

use std::collections::BTreeMap;
use std::fmt::Write;

use serde_json::Value;

use crate::report::{Chart, Direction, Finding, Kpi, Node, Report, Section, Tone};

/// Findings shown per check in the page; the JSON report always has all of them.
const SHOWN_PER_CHECK: usize = 500;

const CSS: &str = r#"
:root{--bg:#F3F5F4;--paper:#FFF;--fg:#17211D;--muted:#5D6A64;--rule:#DCE2DF;--track:#E6EBE8;--accent:#1F6F5C;--accent-soft:#E1F0EB;
--s1:#2E8B57;--s2:#D9932B;--s3:#C0504D;--s4:#9AA6A0;--bar:#2F7F9E;
--ok:#2E8B57;--warn:#C27C12;--bad:#C0504D;--neutral:#2F7F9E;
--sans:"Segoe UI",system-ui,-apple-system,Arial,sans-serif;--mono:Consolas,"SFMono-Regular",monospace}
@media (prefers-color-scheme:dark){:root{--bg:#0F1412;--paper:#18201C;--fg:#E3EAE6;--muted:#97A59E;--rule:#2B3631;--track:#26302B;
--accent:#6CC5AB;--accent-soft:#1D2F29;--s1:#5DBB86;--s2:#E3AE55;--s3:#E07A75;--s4:#6E7A74;--bar:#5AAFD0;
--ok:#5DBB86;--warn:#E3AE55;--bad:#E07A75;--neutral:#5AAFD0}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 var(--sans)}
header{background:var(--paper);border-bottom:1px solid var(--rule);position:sticky;top:0;z-index:5}
.hwrap{max-width:1240px;margin:0 auto;padding:12px 16px;display:flex;flex-wrap:wrap;gap:8px 18px;align-items:center}
.brand{font-weight:700;font-size:1.05rem}.brand span{color:var(--muted);font-weight:400;font-size:.85rem;margin-left:6px}
nav{display:flex;flex-wrap:wrap;gap:6px}nav a{color:var(--fg);text-decoration:none;font-size:.82rem;padding:3px 10px;border:1px solid var(--rule);border-radius:999px}
nav a:hover{border-color:var(--accent);color:var(--accent)}
main{max-width:1240px;margin:0 auto;padding:18px 16px 60px}
.muted{color:var(--muted)}code{font-family:var(--mono);font-size:.88em}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:12px;margin:4px 0 16px}
.kpi{background:var(--paper);border:1px solid var(--rule);border-left:5px solid var(--neutral);border-radius:10px;padding:12px 14px}
.kpi.good{border-left-color:var(--ok)}.kpi.warn{border-left-color:var(--warn)}.kpi.bad{border-left-color:var(--bad)}
.kpi .l{font-size:.8rem;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
.kpi .v{font-size:1.75rem;font-weight:700;line-height:1.2;margin:2px 0}.kpi .d{font-size:.8rem;color:var(--muted)}
.panel{background:var(--paper);border:1px solid var(--rule);border-radius:12px;padding:16px 18px;margin:16px 0}
.panel h2{margin:0 0 10px;font-size:1.15rem;display:flex;align-items:center;gap:10px}
.chip{font-size:.72rem;font-weight:600;padding:2px 9px;border-radius:999px;background:var(--track);color:var(--muted)}
.chip.ok{background:color-mix(in srgb,var(--ok) 18%,transparent);color:var(--ok)}
.chip.bad{background:color-mix(in srgb,var(--bad) 16%,transparent);color:var(--bad)}
.chip.warn{background:color-mix(in srgb,var(--warn) 18%,transparent);color:var(--warn)}
.metrics{display:flex;flex-wrap:wrap;gap:8px;margin:0 0 12px}
.metric{border:1px solid var(--rule);border-radius:8px;padding:6px 10px;min-width:150px}
.metric b{display:block;font-size:1.05rem}.metric span{font-size:.76rem;color:var(--muted)}
.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(330px,1fr));gap:12px;margin:6px 0 10px}
.card{border:1px solid var(--rule);border-radius:10px;padding:12px;min-width:0}
.card h3{margin:0 0 8px;font-size:.9rem}.card.wide{grid-column:1/-1}
.donut{display:flex;align-items:center;gap:14px}.donut .t{font-size:.85rem;color:var(--muted)}
svg text{fill:var(--fg);font-family:var(--sans)}
.legend{display:flex;flex-wrap:wrap;gap:10px;font-size:.78rem;color:var(--muted);margin:0 0 6px}
.legend i{display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:5px;vertical-align:-1px}
.tree .row{display:grid;grid-template-columns:minmax(180px,30%) minmax(120px,1fr) 230px;gap:12px;align-items:center;padding:4px 8px;border-radius:6px}
.tree details>summary{list-style:none;cursor:pointer}.tree details>summary::-webkit-details-marker{display:none}.tree details>summary:hover{background:var(--track)}
.tree .name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.tree details details>summary,.tree details .leaf{border-top:1px dashed var(--rule)}
.tree .caret{display:inline-block;width:14px;color:var(--muted);transition:transform .1s}.tree details[open]>summary .caret{transform:rotate(90deg)}
.tree .num{text-align:right;font-variant-numeric:tabular-nums;color:var(--muted);font-size:.82rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.tree .num b{color:var(--fg);margin-right:6px}
.sbar{display:flex;height:12px;border-radius:3px;overflow:hidden;background:var(--track)}.sbar span{display:block;height:100%}
.hbars{display:grid;grid-template-columns:170px minmax(80px,1fr) 64px;gap:6px 12px;align-items:center;font-size:.82rem}
.hbars .lab{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;text-align:left;color:var(--fg)}
.hbars .trk{height:13px;border-radius:3px;background:var(--track);overflow:hidden;display:flex}
.hbars .trk span{display:block;height:100%}.hbars .n{color:var(--muted);font-variant-numeric:tabular-nums;text-align:right}
table.pivot{width:auto;min-width:60%;margin:0 0 12px}table.pivot td.n,table.pivot th.n{text-align:right;font-variant-numeric:tabular-nums}
table.pivot td:first-child{color:var(--muted)}
.tools{display:flex;gap:8px;margin:0 0 6px}.tools button{font:inherit;font-size:.78rem;border:1px solid var(--rule);background:var(--paper);color:var(--fg);border-radius:6px;padding:2px 8px;cursor:pointer}
details.find{border:1px solid var(--rule);border-radius:8px;margin:8px 0}details.find>summary{cursor:pointer;padding:8px 12px;font-weight:600}
details.find>summary .chip{margin-left:6px}.fbody{padding:0 12px 12px}
.fbody input{width:100%;max-width:360px;font:inherit;padding:5px 9px;border:1px solid var(--rule);border-radius:6px;background:var(--bg);color:var(--fg);margin:0 0 8px}
.wrap{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:.84rem}
th,td{border-bottom:1px solid var(--rule);padding:5px 8px;text-align:left;vertical-align:top}th{color:var(--muted);font-weight:600;position:sticky;top:0;background:var(--paper)}
details.about{margin:6px 0 0;font-size:.85rem;color:var(--muted)}details.about summary{cursor:pointer}
.help{position:relative;display:inline-flex;align-items:center;justify-content:center;width:15px;height:15px;margin-left:5px;
border-radius:50%;border:1px solid var(--muted);color:var(--muted);font-size:10px;font-weight:700;line-height:1;cursor:help;vertical-align:1px;
text-transform:none;letter-spacing:0;flex:none}
.help:hover,.help:focus{border-color:var(--accent);color:var(--accent);outline:none}
.help .tip{display:none;position:absolute;z-index:20;left:50%;top:calc(100% + 7px);transform:translateX(-50%);width:max-content;max-width:300px;
background:var(--fg);color:var(--bg);font-size:.78rem;font-weight:400;line-height:1.45;padding:8px 10px;border-radius:7px;
box-shadow:0 6px 20px rgba(0,0,0,.25);white-space:normal;text-align:left}
.help:hover .tip,.help:focus .tip,.help:focus-within .tip{display:block}
footer{max-width:1240px;margin:0 auto;padding:0 16px 30px;color:var(--muted);font-size:.8rem}
"#;

const JS: &str = r#"
document.querySelectorAll('input[data-filter]').forEach(function(inp){inp.addEventListener('input',function(){
var q=inp.value.toLowerCase();var t=document.getElementById(inp.dataset.filter);if(!t)return;
t.querySelectorAll('tbody tr').forEach(function(r){r.style.display=r.textContent.toLowerCase().indexOf(q)<0?'none':''});});});
document.querySelectorAll('button[data-tree]').forEach(function(b){b.addEventListener('click',function(){
var open=b.dataset.open!=='1';b.dataset.open=open?'1':'0';b.textContent=open?'Collapse all':'Expand all';
document.getElementById(b.dataset.tree).querySelectorAll('details').forEach(function(d){d.open=open});});});
"#;

pub fn render(r: &Report) -> String {
    let mut h = String::with_capacity(1 << 21);
    let _ = write!(
        h,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" \
         content=\"width=device-width,initial-scale=1\"><title>Ontology quality report</title><style>{CSS}</style></head><body>"
    );
    let _ = write!(
        h,
        "<header><div class=\"hwrap\"><div class=\"brand\">Ontology quality report<span>{} {}</span></div><nav>\
         <a href=\"#overview\">Overview</a>",
        r.tool, r.version
    );
    for s in &r.sections {
        let _ = write!(h, "<a href=\"#{}\">{}</a>", s.id, esc(s.title));
    }
    if !r.input_checks.is_empty() || !r.recorded.is_empty() {
        let _ = write!(h, "<a href=\"#record\">Project record</a>");
    }
    let _ = write!(h, "</nav></div></header><main>");

    // Overview
    let _ = write!(h, "<section id=\"overview\"><div class=\"kpis\">");
    for k in &r.overview.kpis {
        kpi(&mut h, k);
    }
    let _ = write!(
        h,
        "</div><div class=\"panel\"><h2>Overview</h2><div class=\"charts\">"
    );
    let mut cards = 0;
    for s in r.sections.iter().filter(|s| s.ran) {
        if let Some(c) = s.charts.iter().find(|c| matches!(c, Chart::Ratio { .. })) {
            chart_card(&mut h, c, &format!("ov-{}", s.id), false);
            cards += 1;
        }
    }
    if cards == 0 {
        let _ = write!(
            h,
            "<div class=\"muted\">No section produced a summary chart.</div>"
        );
    }
    let _ = write!(h, "</div>");
    chart_card(&mut h, &r.overview.findings, "ov-findings", true);
    let _ = write!(h, "</div></section>");

    for s in &r.sections {
        section(&mut h, s);
    }

    if !r.input_checks.is_empty() || !r.recorded.is_empty() {
        let _ = write!(
            h,
            "<section id=\"record\" class=\"panel\"><h2>Project record</h2>"
        );
        if !r.input_checks.is_empty() {
            let passed = r.input_checks.iter().filter(|c| c.ok).count();
            let _ = write!(
                h,
                "<h3>Checks carried by the inputs <span class=\"chip {}\">{passed} of {} passed</span></h3>\
                 <div class=\"wrap\"><table><thead><tr><th>Check</th><th>Result</th><th>Summary</th></tr></thead><tbody>",
                if passed == r.input_checks.len() { "ok" } else { "bad" },
                r.input_checks.len()
            );
            for c in &r.input_checks {
                let (cls, txt) = if c.ok {
                    ("ok", "passed")
                } else {
                    ("bad", "failed")
                };
                let _ = write!(
                    h,
                    "<tr><td>{}</td><td><span class=\"chip {cls}\">{txt}</span></td><td>{}</td></tr>",
                    esc(&c.name),
                    esc(&c.summary)
                );
            }
            let _ = write!(h, "</tbody></table></div>");
        }
        if !r.recorded.is_empty() {
            let _ = write!(
                h,
                "<details class=\"find\"><summary>Problems already recorded in the project<span class=\"chip\">{}</span></summary>\
                 <div class=\"fbody\"><input data-filter=\"t-recorded\" placeholder=\"Filter…\"><div class=\"wrap\"><table id=\"t-recorded\">\
                 <thead><tr><th>Check</th><th>Element</th><th>Detail</th></tr></thead><tbody>",
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
            let _ = write!(h, "</tbody></table></div></div></details>");
        }
        let _ = write!(h, "</section>");
    }
    let _ = write!(
        h,
        "</main><footer>Inputs: {}. The JSON report next to this page has the same content, including every finding.</footer>\
         <script>{JS}</script></body></html>",
        esc(&r.inputs.iter().map(|p| short_path(p)).collect::<Vec<_>>().join(", "))
    );
    h.push('\n');
    h
}

fn kpi(h: &mut String, k: &Kpi) {
    let tone = match k.tone {
        Tone::Good => "good",
        Tone::Warn => "warn",
        Tone::Bad => "bad",
        Tone::Neutral => "",
    };
    let _ = write!(
        h,
        "<div class=\"kpi {tone}\"><div class=\"l\">{}{}</div><div class=\"v\">{}</div><div class=\"d\">{}</div></div>",
        esc(&k.label),
        help_icon(k.help),
        esc(&k.value),
        esc(&k.detail)
    );
}

fn section(h: &mut String, s: &Section) {
    let chip = if !s.ran {
        "<span class=\"chip warn\">not run</span>".to_string()
    } else if s.findings.is_empty() {
        "<span class=\"chip ok\">no findings</span>".to_string()
    } else {
        format!(
            "<span class=\"chip bad\">{} findings</span>",
            crate::thousands(s.findings.len() as u64)
        )
    };
    let _ = write!(
        h,
        "<section id=\"{}\" class=\"panel\"><h2>{} {chip}</h2>",
        s.id,
        esc(s.title)
    );
    if !s.ran {
        for n in &s.notes {
            let _ = write!(h, "<div class=\"muted\">Not run: {}</div>", esc(n));
        }
        let _ = write!(h, "</section>");
        return;
    }
    metrics(h, s);
    let _ = write!(h, "<div class=\"charts\">");
    for (i, c) in s.charts.iter().enumerate() {
        let wide = matches!(c, Chart::Tree { .. });
        chart_card(h, c, &format!("{}-{i}", s.id), wide);
    }
    let _ = write!(h, "</div>");

    let mut by_check: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in &s.findings {
        by_check.entry(f.check.as_str()).or_default().push(f);
    }
    for (n, (check, fs)) in by_check.into_iter().enumerate() {
        let id = format!("t-{}-{n}", s.id);
        let _ = write!(
            h,
            "<details class=\"find\"><summary>{}<span class=\"chip bad\">{}</span></summary><div class=\"fbody\">\
             <input data-filter=\"{id}\" placeholder=\"Filter by term, source, status…\"><div class=\"wrap\"><table id=\"{id}\">\
             <thead><tr><th>Element</th><th>Source</th><th>Status</th><th>Detail</th></tr></thead><tbody>",
            esc(check),
            crate::thousands(fs.len() as u64)
        );
        for f in fs.iter().take(SHOWN_PER_CHECK) {
            let src = f
                .source
                .clone()
                .unwrap_or_else(|| crate::sections::origin_name(f.origin).to_string());
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&f.element),
                esc(&src),
                esc(f.status.as_deref().unwrap_or("")),
                esc(&f.detail)
            );
        }
        let _ = write!(h, "</tbody></table></div>");
        if fs.len() > SHOWN_PER_CHECK {
            let _ = write!(
                h,
                "<div class=\"muted\">Showing {SHOWN_PER_CHECK} of {}; the JSON report has all of them.</div>",
                fs.len()
            );
        }
        let _ = write!(h, "</div></details>");
    }
    if !s.notes.is_empty() {
        let _ = write!(
            h,
            "<details class=\"about\"><summary>About this section</summary><ul>"
        );
        for n in &s.notes {
            let _ = write!(h, "<li>{}</li>", esc(n));
        }
        let _ = write!(h, "</ul></details>");
    }
    let _ = write!(h, "</section>");
}

/// Metrics named "<measure> (<origin>)" are shown as a measure × origin table (the hierarchy in the
/// names); the rest as tiles.
fn metrics(h: &mut String, s: &Section) {
    let mut cols: Vec<String> = Vec::new();
    let mut rows: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    let mut tiles = Vec::new();
    for m in s.metrics.iter().filter(|m| !m.value.is_object()) {
        match m.name.rsplit_once(" (") {
            Some((measure, origin)) if origin.ends_with(')') => {
                let origin = origin.trim_end_matches(')').to_string();
                if !cols.contains(&origin) {
                    cols.push(origin.clone());
                }
                match rows.iter_mut().find(|(r, _)| r == measure) {
                    Some((_, cells)) => {
                        cells.insert(origin, value_text(&m.value));
                    }
                    None => rows.push((
                        measure.to_string(),
                        BTreeMap::from([(origin, value_text(&m.value))]),
                    )),
                }
            }
            _ => tiles.push(m),
        }
    }
    if !rows.is_empty() {
        let _ = write!(
            h,
            "<div class=\"wrap\"><table class=\"pivot\"><thead><tr><th></th>"
        );
        for c in &cols {
            let _ = write!(h, "<th class=\"n\">{}</th>", esc(c));
        }
        let _ = write!(h, "</tr></thead><tbody>");
        for (measure, cells) in &rows {
            let _ = write!(
                h,
                "<tr><td>{}{}</td>",
                esc(measure),
                help_icon(crate::help::about(measure))
            );
            for c in &cols {
                let _ = write!(
                    h,
                    "<td class=\"n\">{}</td>",
                    esc(cells.get(c).map(String::as_str).unwrap_or("–"))
                );
            }
            let _ = write!(h, "</tr>");
        }
        let _ = write!(h, "</tbody></table></div>");
    }
    if !tiles.is_empty() {
        let _ = write!(h, "<div class=\"metrics\">");
        for m in tiles {
            let _ = write!(
                h,
                "<div class=\"metric\"><b>{}</b><span>{}{}</span></div>",
                esc(&value_text(&m.value)),
                esc(&m.name),
                help_icon(m.help)
            );
        }
        let _ = write!(h, "</div>");
    }
}

// ── charts ─────────────────────────────────────────────────────────────────

fn chart_card(h: &mut String, c: &Chart, id: &str, wide: bool) {
    let title = match c {
        Chart::Bars { title, .. }
        | Chart::Stacked { title, .. }
        | Chart::Ratio { title, .. }
        | Chart::Tree { title, .. } => title,
    };
    let _ = write!(
        h,
        "<div class=\"card{}\"><h3>{}{}</h3>",
        if wide { " wide" } else { "" },
        esc(title),
        help_icon(crate::help::about(title))
    );
    match c {
        Chart::Ratio {
            part,
            whole,
            direction,
            ..
        } => donut(h, *part, *whole, *direction),
        Chart::Bars { items, .. } => {
            let max = items.iter().map(|i| i.value).max().unwrap_or(1).max(1);
            hbars(
                h,
                items.iter().map(|i| (i.label.as_str(), vec![i.value])),
                max,
                &[],
                &[],
            );
        }
        Chart::Stacked { series, rows, .. } => {
            let active: Vec<usize> = (0..series.len())
                .filter(|&i| {
                    rows.iter()
                        .any(|r| r.values.get(i).copied().unwrap_or(0) > 0)
                })
                .collect();
            let shown: Vec<String> = active.iter().map(|&i| series[i].clone()).collect();
            legend_at(h, &shown, &active);
            let max = rows
                .iter()
                .map(|r| r.values.iter().sum::<u64>())
                .max()
                .unwrap_or(1)
                .max(1);
            hbars(
                h,
                rows.iter().map(|r| {
                    let v: Vec<u64> = active
                        .iter()
                        .map(|&i| r.values.get(i).copied().unwrap_or(0))
                        .collect();
                    (r.label.as_str(), v)
                }),
                max,
                &shown,
                &active,
            );
        }
        Chart::Tree { series, roots, .. } => tree(h, series, roots, id),
    }
    let _ = write!(h, "</div>");
}

fn donut(h: &mut String, part: u64, whole: u64, direction: Direction) {
    if whole == 0 {
        let _ = write!(h, "<div class=\"muted\">Nothing to measure.</div>");
        return;
    }
    let share = part as f64 / whole as f64;
    let color = match direction {
        Direction::Neutral => "var(--neutral)",
        d => {
            let good = if matches!(d, Direction::HigherIsBetter) {
                share
            } else {
                1.0 - share
            };
            if good >= 0.95 {
                "var(--ok)"
            } else if good >= 0.8 {
                "var(--warn)"
            } else {
                "var(--bad)"
            }
        }
    };
    let r = 46.0;
    let circ = 2.0 * std::f64::consts::PI * r;
    let dash = circ * share;
    let _ = write!(
        h,
        "<div class=\"donut\"><svg width=\"120\" height=\"120\" viewBox=\"0 0 120 120\" role=\"img\">\
         <title>{part} of {whole}</title>\
         <circle cx=\"60\" cy=\"60\" r=\"{r}\" fill=\"none\" stroke=\"var(--track)\" stroke-width=\"14\"/>\
         <circle cx=\"60\" cy=\"60\" r=\"{r}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"14\" \
         stroke-dasharray=\"{dash:.2} {circ:.2}\" transform=\"rotate(-90 60 60)\" stroke-linecap=\"butt\"/>\
         <text x=\"60\" y=\"65\" text-anchor=\"middle\" style=\"font-size:19px;font-weight:700\">{:.1}%</text></svg>\
         <div class=\"t\"><b style=\"color:var(--fg)\">{}</b> of {}</div></div>",
        100.0 * share,
        crate::thousands(part),
        crate::thousands(whole)
    );
}

/// Horizontal bars as HTML (legible at any width). One series draws a single bar; several draw a
/// stacked bar. All rows share the scale `max`.
fn hbars<'a, I: Iterator<Item = (&'a str, Vec<u64>)>>(
    h: &mut String,
    rows: I,
    max: u64,
    series: &[String],
    colors: &[usize],
) {
    let mut any = false;
    let _ = write!(h, "<div class=\"hbars\">");
    for (label, values) in rows {
        any = true;
        let total: u64 = values.iter().sum();
        let tip = if series.is_empty() {
            format!("{label}: {}", crate::thousands(total))
        } else {
            let parts: Vec<String> = series
                .iter()
                .zip(&values)
                .map(|(s, v)| format!("{s} {}", crate::thousands(*v)))
                .collect();
            format!("{label}: {}", parts.join(", "))
        };
        let _ = write!(
            h,
            "<span class=\"lab\" title=\"{0}\">{1}</span><span class=\"trk\" title=\"{0}\">",
            esc(&tip),
            esc(label)
        );
        for (i, v) in values.iter().enumerate() {
            if *v > 0 {
                let color = if series.is_empty() {
                    "var(--bar)"
                } else {
                    series_color(colors.get(i).copied().unwrap_or(i))
                };
                let _ = write!(
                    h,
                    "<span style=\"width:{:.2}%;background:{color}\"></span>",
                    100.0 * *v as f64 / max as f64
                );
            }
        }
        let _ = write!(
            h,
            "</span><span class=\"n\">{}</span>",
            crate::thousands(total)
        );
    }
    let _ = write!(h, "</div>");
    if !any {
        let _ = write!(h, "<div class=\"muted\">None.</div>");
    }
}

fn series_color(i: usize) -> &'static str {
    ["var(--s1)", "var(--s2)", "var(--s3)", "var(--s4)"][i.min(3)]
}

fn tree(h: &mut String, series: &[String], roots: &[Node], id: &str) {
    if roots.is_empty() {
        let _ = write!(h, "<div class=\"muted\">None.</div>");
        return;
    }
    // Series that are zero everywhere (e.g. "other") are not drawn or listed.
    let mut active: Vec<usize> = (0..series.len())
        .filter(|&i| roots.iter().any(|n| any_nonzero(n, i)))
        .collect();
    if active.is_empty() {
        active = (0..series.len()).collect();
    }
    let shown: Vec<String> = active.iter().map(|&i| series[i].clone()).collect();
    let _ = write!(
        h,
        "<div class=\"tools\"><button type=\"button\" data-tree=\"{id}\">Expand all</button></div>"
    );
    if shown.len() > 1 {
        legend_at(h, &shown, &active);
    }
    let _ = write!(h, "<div class=\"tree\" id=\"{id}\">");
    let max = level_max(roots);
    for n in roots {
        tree_node(h, &shown, &active, n, max, 0);
    }
    let _ = write!(h, "</div>");
}

fn any_nonzero(n: &Node, i: usize) -> bool {
    n.values.get(i).copied().unwrap_or(0) > 0 || n.children.iter().any(|c| any_nonzero(c, i))
}

/// Bars are scaled against the largest node among siblings, so every level of the drill-down is legible.
fn level_max(nodes: &[Node]) -> u64 {
    nodes
        .iter()
        .map(|n| n.values.iter().sum::<u64>())
        .max()
        .unwrap_or(1)
        .max(1)
}

fn tree_node(h: &mut String, shown: &[String], active: &[usize], n: &Node, max: u64, depth: usize) {
    let total: u64 = n.values.iter().sum();
    let mut bar = String::from("<div class=\"sbar\">");
    for (k, &i) in active.iter().enumerate() {
        let v = n.values.get(i).copied().unwrap_or(0);
        if v > 0 {
            let pct = 100.0 * v as f64 / max as f64;
            let color = if shown.len() > 1 {
                series_color(i)
            } else {
                "var(--bar)"
            };
            let _ = write!(
                bar,
                "<span style=\"width:{pct:.2}%;background:{color}\" title=\"{} {}\"></span>",
                esc(&shown[k]),
                crate::thousands(v)
            );
        }
    }
    bar.push_str("</div>");
    let nums = if shown.len() > 1 {
        format!(
            "<b>{}</b> <span title=\"{}\">{}</span>",
            crate::thousands(total),
            esc(&shown.join(" / ")),
            active
                .iter()
                .map(|&i| crate::thousands(n.values.get(i).copied().unwrap_or(0)))
                .collect::<Vec<_>>()
                .join(" / ")
        )
    } else {
        format!("<b>{}</b>", crate::thousands(total))
    };
    let indent = format!("padding-left:{}px", depth * 18);
    if n.children.is_empty() {
        let _ = write!(
            h,
            "<div class=\"row leaf\"><span class=\"name\" style=\"{indent}\" title=\"{0}\"><span class=\"caret\"></span>{0}</span>{bar}<span class=\"num\">{nums}</span></div>",
            esc(&n.label)
        );
        return;
    }
    let _ = write!(
        h,
        "<details><summary class=\"row\"><span class=\"name\" style=\"{indent}\" title=\"{0}\"><span class=\"caret\">▸</span>{0}</span>{bar}<span class=\"num\">{nums}</span></summary>",
        esc(&n.label)
    );
    let kid_max = level_max(&n.children);
    for c in &n.children {
        tree_node(h, shown, active, c, kid_max, depth + 1);
    }
    let _ = write!(h, "</details>");
}

/// A legend for a subset of series, keeping each series' own colour.
fn legend_at(h: &mut String, shown: &[String], active: &[usize]) {
    let _ = write!(h, "<div class=\"legend\">");
    for (s, &i) in shown.iter().zip(active) {
        let _ = write!(
            h,
            "<span><i style=\"background:{}\"></i>{}</span>",
            series_color(i),
            esc(s)
        );
    }
    let _ = write!(h, "</div>");
}

// ── text helpers ───────────────────────────────────────────────────────────

/// A (?) icon that shows the explanation on hover, keyboard focus or tap (no JavaScript needed).
fn help_icon(text: Option<&str>) -> String {
    match text {
        Some(t) => format!(
            "<span class=\"help\" tabindex=\"0\" role=\"button\" aria-label=\"{0}\">?<span class=\"tip\" role=\"tooltip\">{0}</span></span>",
            esc(t)
        ),
        None => String::new(),
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n
            .as_u64()
            .map(crate::thousands)
            .unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Only the last two path components, so the page does not expose a machine's folder layout.
pub(crate) fn short_path(p: &str) -> String {
    let parts: Vec<&str> = p.split(['/', '\\']).filter(|x| !x.is_empty()).collect();
    parts[parts.len().saturating_sub(2)..].join("/")
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
