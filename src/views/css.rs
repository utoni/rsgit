pub const CSS: &str = r#"
:root{
  color-scheme:dark;
  --bg:#121316; --bg-elev:#1a1c21; --panel:#1e2127; --code-bg:#16181d;
  --ink:#e8e5df; --muted:#9d968b; --faint:#6c675f;
  --rule:#2a2e35; --rule-strong:#3b4048;
  --oxide:#e28a5b; --oxide-dim:#c9784e;
  --add-bg:#15271d; --add-ink:#82d097; --del-bg:#2c171a; --del-ink:#e78d8d;
  --mono:ui-monospace,"SFMono-Regular","JetBrains Mono",Menlo,Consolas,monospace;
  --sans:system-ui,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
}
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%;background:var(--bg)}
body{
  margin:0;background:var(--bg);color:var(--ink);
  font-family:var(--mono);font-size:14px;line-height:1.55;
  -webkit-font-smoothing:antialiased;
}
a{color:var(--oxide);text-decoration:none}
a:hover{text-decoration:underline}
a:focus-visible,button:focus-visible{outline:2px solid var(--oxide);outline-offset:2px}

/* --- Top bar / wordmark --- */
.topbar{border-top:3px solid var(--oxide);background:var(--bg-elev);
  border-bottom:1px solid var(--rule)}
.topbar .inner{max-width:1080px;margin:0 auto;padding:10px 14px;display:flex;
  align-items:baseline;gap:10px;flex-wrap:wrap}
.wordmark{font-weight:700;font-size:16px;letter-spacing:.02em;color:var(--ink)}
.wordmark a{color:inherit}
.crumbs{color:var(--muted);font-size:13px;word-break:break-all}
.crumbs a{color:var(--oxide)}
.crumbs .sep{color:var(--faint);padding:0 4px}
.tagline{display:none;color:var(--faint);font-size:11px;font-family:var(--sans)}

/* --- Tabs --- */
.tabs{max-width:1080px;margin:0 auto;padding:0 8px;display:flex;gap:2px;
  border-bottom:1px solid var(--rule);overflow-x:auto;-webkit-overflow-scrolling:touch}
.tabs::-webkit-scrollbar{height:0}
.tabs a{padding:11px 14px;color:var(--muted);border-bottom:2px solid transparent;
  font-size:13px;text-transform:lowercase;letter-spacing:.03em;white-space:nowrap}
.tabs a:hover{color:var(--ink);text-decoration:none}
.tabs a.active{color:var(--ink);border-bottom-color:var(--oxide);font-weight:700}

main{max-width:1080px;margin:0 auto;padding:16px 14px 56px}
.section-title{font-size:12px;text-transform:uppercase;letter-spacing:.08em;
  color:var(--muted);margin:24px 0 8px;font-weight:700}
.section-title:first-child{margin-top:2px}

/* --- Tables --- */
table.grid{display:block;overflow-x:auto;-webkit-overflow-scrolling:touch;
  width:100%;border-collapse:collapse;font-size:13px}
table.grid th{text-align:left;font-weight:700;color:var(--muted);
  font-size:11px;text-transform:uppercase;letter-spacing:.05em;
  padding:7px 10px;border-bottom:1px solid var(--rule-strong);white-space:nowrap}
table.grid td{padding:7px 10px;border-bottom:1px solid var(--rule);vertical-align:top}
table.grid tr:hover td{background:var(--panel)}
.num{text-align:right;color:var(--faint);font-variant-numeric:tabular-nums}
.nowrap{white-space:nowrap}
.desc{color:var(--muted);font-family:var(--sans)}
.mono-id{color:var(--oxide-dim)}
.mut{color:var(--muted)}
.faint{color:var(--faint)}
.icon{display:inline-block;width:1.1em;color:var(--faint)}
.icon.dir{color:var(--oxide-dim)}

/* --- README --- */
.readme{border:1px solid var(--rule);background:var(--bg-elev);border-radius:6px;
  padding:18px 18px;margin-top:8px;font-family:var(--sans);font-size:15px;
  line-height:1.65;color:var(--ink);overflow-wrap:break-word}
.readme h1,.readme h2,.readme h3{line-height:1.25;margin:1.2em 0 .5em}
.readme h1{font-size:1.5em;border-bottom:1px solid var(--rule);padding-bottom:.2em}
.readme h2{font-size:1.25em;border-bottom:1px solid var(--rule);padding-bottom:.2em}
.readme pre{background:var(--code-bg);padding:12px 14px;border-radius:6px;overflow:auto;
  font-family:var(--mono);font-size:13px}
.readme code{background:var(--panel);padding:.12em .35em;border-radius:4px;
  font-family:var(--mono);font-size:.88em}
.readme pre code{background:none;padding:0}
.readme a{color:var(--oxide)}
.readme table{border-collapse:collapse;display:block;overflow-x:auto}
.readme td,.readme th{border:1px solid var(--rule);padding:5px 9px}
.readme img{max-width:100%;height:auto}
.readme blockquote{margin:.8em 0;padding-left:12px;border-left:3px solid var(--rule-strong);color:var(--muted)}

/* --- Source blob --- */
.blobhead{display:flex;justify-content:space-between;align-items:center;gap:10px;
  border:1px solid var(--rule);border-bottom:none;background:var(--panel);
  padding:8px 12px;font-size:12px;border-radius:6px 6px 0 0;flex-wrap:wrap}
.code{border:1px solid var(--rule);border-radius:0 0 6px 6px;overflow:auto;
  background:var(--code-bg);-webkit-overflow-scrolling:touch}
table.src{border-collapse:collapse;width:100%;font-size:12.5px}
table.src td{vertical-align:top}
table.src td.ln{text-align:right;color:var(--faint);padding:0 10px;user-select:none;
  width:1%;white-space:nowrap;border-right:1px solid var(--rule);
  font-variant-numeric:tabular-nums;position:sticky;left:0;background:var(--code-bg)}
table.src td.ln a{color:var(--faint)}
table.src td.cl{padding:0 14px;white-space:pre;width:100%}
table.src tr:target td{background:#2a2410}
table.src tr:target td.ln{background:#39310f}

/* --- Diff --- */
.filediff{border:1px solid var(--rule);border-radius:6px;margin:14px 0;overflow:hidden}
.filediff .fh{background:var(--panel);padding:8px 12px;font-size:12px;
  display:flex;gap:10px;align-items:center;flex-wrap:wrap;
  border-bottom:1px solid var(--rule);word-break:break-all}
.badge{font-size:10px;text-transform:uppercase;letter-spacing:.04em;
  padding:1px 6px;border-radius:3px;font-weight:700}
.badge.A{background:var(--add-bg);color:var(--add-ink)}
.badge.D{background:var(--del-bg);color:var(--del-ink)}
.badge.M{background:#182a3f;color:#7fb0ee}
.badge.R{background:#241a3a;color:#b295e6}
.stat-add{color:var(--add-ink)}
.stat-del{color:var(--del-ink)}
.diffwrap{overflow-x:auto;-webkit-overflow-scrolling:touch}
table.diff{border-collapse:collapse;width:100%;font-size:12.5px}
table.diff td{padding:0 8px;white-space:pre;vertical-align:top}
table.diff td.no{text-align:right;color:var(--faint);user-select:none;width:1%;
  white-space:nowrap;border-right:1px solid var(--rule);font-variant-numeric:tabular-nums}
tr.add td{background:var(--add-bg)} tr.add td.cl{color:var(--add-ink)}
tr.del td{background:var(--del-bg)} tr.del td.cl{color:var(--del-ink)}
tr.hsep td{background:var(--panel);color:var(--faint);padding:2px 8px;font-size:11px}
.note{color:var(--muted);padding:10px 12px;font-style:italic}

/* --- Commit meta --- */
.meta{border:1px solid var(--rule);border-radius:6px;background:var(--bg-elev);
  padding:14px 16px;margin-bottom:8px;overflow-wrap:break-word}
.meta .subject{font-size:15px;font-weight:700;margin-bottom:10px;font-family:var(--sans)}
.meta dl{display:grid;grid-template-columns:max-content 1fr;gap:4px 14px;margin:0}
.meta dt{color:var(--muted)}
.meta dd{margin:0;word-break:break-all}

.pager{margin-top:18px;display:flex;gap:14px;font-size:13px}

footer{max-width:1080px;margin:0 auto;padding:18px 14px;color:var(--faint);
  font-size:11px;border-top:1px solid var(--rule);font-family:var(--sans)}

/* --- Wider screens --- */
@media (min-width:640px){
  body{font-size:13px}
  .tagline{display:block;margin-left:auto}
  .topbar .inner{padding:12px 20px;gap:14px}
  .tabs{padding:0 20px}
  main{padding:22px 20px 60px}
  .readme{padding:22px 26px}
  table.grid{display:table}
}
@media (min-width:900px){
  .meta dl{gap:3px 16px}
}
@media (prefers-reduced-motion:reduce){*{transition:none!important}}
"#;
