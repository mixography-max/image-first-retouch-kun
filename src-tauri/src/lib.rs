pub mod report;
pub mod retouch;

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

#[derive(serde::Serialize, Clone)]
pub struct ProgressEvent {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
    pub stats: Option<retouch::RetouchStats>,
}

#[derive(serde::Serialize)]
pub struct RetouchResult {
    pub output_dir: String,
    pub stats: Vec<retouch::RetouchStats>,
    pub total: usize,
    pub succeeded: usize,
}

fn is_supported_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase());
    matches!(ext.as_deref(), Some("jpg") | Some("jpeg") | Some("png"))
}

#[tauri::command]
async fn process_images(
    app: AppHandle,
    paths: Vec<String>,
    target_l: Option<f32>,
) -> Result<RetouchResult, String> {
    let target = target_l.unwrap_or(122.0);
    let mut file_pairs: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut primary_output_dir: Option<PathBuf> = None;

    for p_str in paths {
        let p = PathBuf::from(&p_str);
        if p.is_file() {
            if is_supported_image(&p) {
                let out_dir = p
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("first_retouched");
                if primary_output_dir.is_none() {
                    primary_output_dir = Some(out_dir.clone());
                }
                let out_file = out_dir.join(p.file_name().unwrap());
                file_pairs.push((p, out_file));
            }
        } else if p.is_dir() {
            let out_dir = p.join("first_retouched");
            if primary_output_dir.is_none() {
                primary_output_dir = Some(out_dir.clone());
            }
            for entry in WalkDir::new(&p).min_depth(1).max_depth(1) {
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
        return Err("処理可能な画像（.jpg, .jpeg, .png）が見つかりませんでした。".into());
    }

    let total = file_pairs.len();
    let mut stats_list = Vec::new();

    for (idx, (in_path, out_path)) in file_pairs.iter().enumerate() {
        let file_name = in_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let _ = app.emit(
            "retouch-progress",
            ProgressEvent {
                current: idx + 1,
                total,
                file_name: file_name.clone(),
                stats: None,
            },
        );

        match retouch::harmonize_file(in_path, out_path, target) {
            Ok(stat) => {
                let _ = app.emit(
                    "retouch-progress",
                    ProgressEvent {
                        current: idx + 1,
                        total,
                        file_name: file_name.clone(),
                        stats: Some(stat.clone()),
                    },
                );
                stats_list.push(stat);
            }
            Err(e) => {
                eprintln!("Error processing {:?}: {}", in_path, e);
            }
        }
    }

    let output_dir_str = if let Some(ref out_dir) = primary_output_dir {
        let _ = report::generate_comparison_html(out_dir, &stats_list, "..");
        out_dir.to_string_lossy().to_string()
    } else {
        "".into()
    };

    Ok(RetouchResult {
        output_dir: output_dir_str,
        succeeded: stats_list.len(),
        total,
        stats: stats_list,
    })
}

#[tauri::command]
fn open_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![process_images, open_in_folder])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

