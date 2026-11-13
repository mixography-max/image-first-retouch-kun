//! CLI binary: first-retouch
//! Command-line interface for batch photo harmonization.

mod report;
mod retouch;

use clap::Parser;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(
    name = "first-retouch",
    about = "🐹📷 画像ファーストレタッチくん — 色相保全明度調和 CLI",
    version = "1.0.0"
)]
struct Args {
    /// Input file(s) or directory
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// Output directory (default: 'first_retouched/' subdirectory next to input)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Target median L* luminance (default: 122.0)
    #[arg(short, long, default_value_t = 122.0)]
    target: f32,

    /// Generate comparison HTML viewer in output directory
    #[arg(long, default_value_t = true)]
    report: bool,
}

fn is_supported_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());
    matches!(
        ext.as_deref(),
        Some("jpg") | Some("jpeg") | Some("png")
    )
}

fn main() {
    let args = Args::parse();

    println!("============================================================");
    println!(" 🐹📷 画像ファーストレタッチくん (Photo First-Retouch Kun) CLI");
    println!(" Target Median L*: {:.1}", args.target);
    println!("============================================================");

    // Collect all image files
    let mut file_pairs: Vec<(PathBuf, PathBuf)> = Vec::new();

    for input in &args.inputs {
        if input.is_file() {
            if is_supported_image(input) {
                let out_dir = match &args.output {
                    Some(dir) => dir.clone(),
                    None => input
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join("first_retouched"),
                };
                let out_file = out_dir.join(input.file_name().unwrap());
                file_pairs.push((input.clone(), out_file));
            }
        } else if input.is_dir() {
            let out_dir = match &args.output {
                Some(dir) => dir.clone(),
                None => input.join("first_retouched"),
            };
            for entry in WalkDir::new(input).min_depth(1).max_depth(1) {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() && is_supported_image(path) {
                        let out_file = out_dir.join(path.file_name().unwrap());
                        file_pairs.push((path.to_path_buf(), out_file));
                    }
                }
            }
        }
    }

    if file_pairs.is_empty() {
        eprintln!("No supported image files found (.jpg, .jpeg, .png)");
        std::process::exit(1);
    }

    println!("Found {} photos. Processing...", file_pairs.len());

    let target_l = args.target;

    // Process in parallel using Rayon
    let stats: Vec<retouch::RetouchStats> = file_pairs
        .par_iter()
        .filter_map(|(in_path, out_path)| {
            match retouch::harmonize_file(in_path, out_path, target_l) {
                Ok(stat) => {
                    println!(
                        "  [OK] {:30} | L* Med: {:5.1} -> {:5.1} | &sigma;: {:4.1} -> {:4.1} | &gamma;={:.2}",
                        stat.file_name, stat.med_pre, stat.med_post, stat.std_pre, stat.std_post, stat.gamma
                    );
                    Some(stat)
                }
                Err(err) => {
                    eprintln!("  [ERR] {:?}: {}", in_path.file_name(), err);
                    None
                }
            }
        })
        .collect();

    println!("============================================================");
    println!(" Successfully processed {} / {} photos!", stats.len(), file_pairs.len());

    // Generate comparison HTML if requested
    if args.report && !stats.is_empty() {
        if let Some((_, first_out)) = file_pairs.first() {
            if let Some(out_dir) = first_out.parent() {
                let _ = report::generate_comparison_html(out_dir, &stats, "..");
                println!(" Report viewer: {}", out_dir.join("comparison_viewer.html").display());
            }
        }
    }
    println!("============================================================");
}

