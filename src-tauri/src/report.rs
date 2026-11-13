//! Generates bilingual (Japanese / English) comparison viewer HTML

use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::retouch::RetouchStats;

pub fn generate_comparison_html(
    output_dir: &Path,
    stats_list: &[RetouchStats],
    rel_original_prefix: &str,
) -> std::io::Result<()> {
    let mut cards_html = String::new();

    for stat in stats_list {
        let file_name = &stat.file_name;
        let delta_m = stat.med_post - stat.med_pre;
        let delta_s = stat.std_post - stat.std_pre;

        cards_html.push_str(&format!(
            r#"
        <div class="card" id="{file_name}">
            <div class="card-header">
                <span class="file-title">{file_name}</span>
                <span class="badge">L* Med: {m_pre:.0} → <strong>{m_post:.0}</strong> ({delta_m:+.1})</span>
                <span class="badge">Contrast &sigma;: {s_pre:.1} → <strong>{s_post:.1}</strong> ({delta_s:+.1})</span>
                <span class="badge">&gamma;: {gamma:.2}</span>
            </div>
            <div class="comparison-container">
                <div class="img-box">
                    <div class="img-label before-label">◀ 調整前 / Original</div>
                    <img src="{rel_original_prefix}/{file_name}" alt="{file_name} Original" loading="lazy">
                </div>
                <div class="img-box">
                    <div class="img-label after-label">調整後 / Harmonized ▶</div>
                    <img src="./{file_name}" alt="{file_name} Harmonized" loading="lazy">
                </div>
            </div>
        </div>
        "#,
            file_name = file_name,
            m_pre = stat.med_pre,
            m_post = stat.med_post,
            delta_m = delta_m,
            s_pre = stat.std_pre,
            s_post = stat.std_post,
            delta_s = delta_s,
            gamma = stat.gamma,
            rel_original_prefix = rel_original_prefix
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>画像ファーストレタッチくん 🐹📷 | 比較ビューア</title>
<style>
:root {{
    --bg-color: #121212;
    --card-bg: #1c1c1e;
    --border-color: #2e2e32;
    --text-primary: #f0f0f5;
    --text-secondary: #a0a0ab;
    --accent: #3b82f6;
    --accent-badge: #1e293b;
    --accent-text: #93c5fd;
}}
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    background-color: var(--bg-color);
    color: var(--text-primary);
    line-height: 1.6;
    padding: 24px;
}}
header {{
    max-width: 1540px;
    margin: 0 auto 28px auto;
    padding-bottom: 24px;
    border-bottom: 1px solid var(--border-color);
}}
h1 {{
    font-size: 24px;
    font-weight: 700;
    color: #ffffff;
}}
.subtitle {{
    font-size: 14px;
    color: var(--text-secondary);
    margin-top: 4px;
}}
.intro-card {{
    background: #182230;
    border-left: 4px solid var(--accent);
    padding: 16px;
    border-radius: 6px;
    font-size: 13.5px;
    color: #d1d5db;
    margin-top: 16px;
}}
.main-content {{
    max-width: 1540px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 36px;
}}
.card {{
    background: var(--card-bg);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 20px;
    box-shadow: 0 4px 20px rgba(0,0,0,0.45);
}}
.card-header {{
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 14px;
}}
.file-title {{
    font-size: 18px;
    font-weight: 600;
    color: #ffffff;
    font-family: ui-monospace, monospace;
}}
.badge {{
    font-size: 12.5px;
    background: var(--accent-badge);
    color: var(--accent-text);
    padding: 3px 10px;
    border-radius: 14px;
    border: 1px solid #2d3e56;
}}
.comparison-container {{
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
}}
@media (max-width: 900px) {{
    .comparison-container {{
        grid-template-columns: 1fr;
    }}
}}
.img-box {{
    position: relative;
    background: #000;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid #2a2a2e;
}}
.img-label {{
    position: absolute;
    top: 10px;
    padding: 5px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    z-index: 10;
}}
.before-label {{ left: 10px; background: rgba(220, 38, 38, 0.88); color: #fff; }}
.after-label {{ right: 10px; background: rgba(16, 185, 129, 0.88); color: #fff; }}
.img-box img {{
    width: 100%;
    height: auto;
    display: block;
    image-orientation: from-image;
}}
footer {{
    max-width: 1540px;
    margin: 48px auto 0 auto;
    text-align: center;
    color: #71717a;
    font-size: 13px;
    border-top: 1px solid var(--border-color);
    padding-top: 24px;
}}
</style>
</head>
<body>
<header>
    <h1>画像ファーストレタッチくん 🐹📷 比較ビューア</h1>
    <div class="subtitle">Photo First-Retouch Kun — Before / After Comparison Report</div>
    <div class="intro-card">
        <strong>調整方針（色相完全保全）:</strong>
        CIE L*a*b* 色空間の明度（L*）チャネルのみを調整し、色彩（a*, b*）を一切変更せず保持（色相・色温度不変）。
        適応型ガンマと適応型CLAHEにより、白飛びや黒つぶれを抑え、微細テクスチャの陰影を鮮明化しました。
    </div>
</header>
<main class="main-content">
{cards}
</main>
<footer>
    Generated by 画像ファーストレタッチくん (Photo First-Retouch Kun) • Pure Rust High-Fidelity Engine
</footer>
</body>
</html>"#,
        cards = cards_html
    );

    let html_path = output_dir.join("comparison_viewer.html");
    let mut file = File::create(html_path)?;
    file.write_all(html.as_bytes())?;
    Ok(())
}

