import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "framer-motion";
import "./App.css";

// ── Types ──────────────────────────────────────────────────────────────────

interface RetouchStats {
  file_name: string;
  width: number;
  height: number;
  med_pre: number;
  med_post: number;
  std_pre: number;
  std_post: number;
  gamma: number;
  clip_limit: number;
}

interface ProgressEvent {
  current: number;
  total: number;
  file_name: string;
  stats: RetouchStats | null;
}

interface RetouchResult {
  output_dir: string;
  stats: RetouchStats[];
  total: number;
  succeeded: number;
}

type Phase = "idle" | "processing" | "done" | "error";

// ── Hamster mascot SVG ───────────────────────────────────────────────────

const HamsterMascot = ({ size = 90 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 120 120" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
    {/* Body */}
    <ellipse cx="60" cy="78" rx="38" ry="30" fill="#c9955c"/>
    {/* Head */}
    <circle cx="60" cy="50" r="30" fill="#d4a56a"/>
    {/* Cheeks */}
    <ellipse cx="35" cy="58" rx="13" ry="11" fill="#e8b89e" opacity=".7"/>
    <ellipse cx="85" cy="58" rx="13" ry="11" fill="#e8b89e" opacity=".7"/>
    {/* Ears */}
    <circle cx="30" cy="27" r="13" fill="#c9955c"/>
    <circle cx="30" cy="27" r="8" fill="#f0b8a8"/>
    <circle cx="90" cy="27" r="13" fill="#c9955c"/>
    <circle cx="90" cy="27" r="8" fill="#f0b8a8"/>
    {/* Eyes */}
    <circle cx="47" cy="46" r="5.5" fill="#2d1a0a"/>
    <circle cx="73" cy="46" r="5.5" fill="#2d1a0a"/>
    <circle cx="49" cy="44" r="2" fill="white"/>
    <circle cx="75" cy="44" r="2" fill="white"/>
    {/* Nose */}
    <ellipse cx="60" cy="57" rx="4" ry="3" fill="#c26b5a"/>
    {/* Mouth */}
    <path d="M55 61 Q60 66 65 61" stroke="#8b4513" strokeWidth="1.5" strokeLinecap="round" fill="none"/>
    {/* Camera body */}
    <rect x="34" y="80" width="52" height="34" rx="6" fill="#2a2a2a"/>
    <rect x="38" y="84" width="44" height="26" rx="4" fill="#1a1a1a"/>
    {/* Lens ring */}
    <circle cx="60" cy="97" r="11" fill="#333"/>
    <circle cx="60" cy="97" r="8" fill="#111"/>
    <circle cx="60" cy="97" r="5" fill="#1e3a5f"/>
    <circle cx="58" cy="95" r="2" fill="white" opacity=".4"/>
    {/* Shutter button */}
    <circle cx="78" cy="82" r="3.5" fill="#f59e0b"/>
    {/* Flash */}
    <rect x="38" y="82" width="8" height="5" rx="2" fill="#f59e0b" opacity=".8"/>
    {/* Paws holding camera */}
    <ellipse cx="34" cy="96" rx="7" ry="6" fill="#d4a56a"/>
    <ellipse cx="86" cy="96" rx="7" ry="6" fill="#d4a56a"/>
  </svg>
);

// ── Delta badge ───────────────────────────────────────────────────────────

const Delta = ({ label, pre, post }: { label: string; pre: number; post: number }) => {
  const diff = post - pre;
  const isPositive = diff > 0;
  return (
    <span className="stat-badge">
      <span className="stat-label">{label}</span>
      <span className="stat-pre">{pre.toFixed(1)}</span>
      <span className="stat-arrow">→</span>
      <span className="stat-post">{post.toFixed(1)}</span>
      <span className={`stat-delta ${isPositive ? "pos" : diff < 0 ? "neg" : "zero"}`}>
        {diff > 0 ? "+" : ""}{diff.toFixed(1)}
      </span>
    </span>
  );
};

// ── Result Card ────────────────────────────────────────────────────────────

const StatCard = ({ stat }: { stat: RetouchStats }) => (
  <motion.div
    className="stat-card"
    initial={{ opacity: 0, y: 12 }}
    animate={{ opacity: 1, y: 0 }}
    transition={{ duration: 0.25 }}
  >
    <span className="stat-filename">{stat.file_name}</span>
    <span className="stat-dim">{stat.width}×{stat.height}</span>
    <div className="stat-badges">
      <Delta label="L* Med" pre={stat.med_pre} post={stat.med_post} />
      <Delta label="σ" pre={stat.std_pre} post={stat.std_post} />
      <span className="stat-badge"><span className="stat-label">γ</span>{stat.gamma.toFixed(2)}</span>
    </div>
  </motion.div>
);

// ── App ────────────────────────────────────────────────────────────────────

export default function App() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [isDragOver, setIsDragOver] = useState(false);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [result, setResult] = useState<RetouchResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string>("");
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const phaseRef = useRef<Phase>(phase);
  phaseRef.current = phase;

  const reset = useCallback(() => {
    setPhase("idle");
    setIsDragOver(false);
    setProgress(null);
    setResult(null);
    setErrorMsg("");
  }, []);

  const startProcessing = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    setPhase("processing");
    setProgress(null);
    setResult(null);

    // Subscribe to progress events from Rust
    const unlisten = await listen<ProgressEvent>("retouch-progress", (event) => {
      setProgress(event.payload);
    });
    unlistenRef.current = unlisten;

    try {
      const res = await invoke<RetouchResult>("process_images", {
        paths,
        targetL: 122.0,
      });
      setResult(res);
      setPhase("done");
    } catch (err) {
      setErrorMsg(String(err));
      setPhase("error");
    } finally {
      unlistenRef.current?.();
      unlistenRef.current = null;
    }
  }, []);

  // Tauri v2 native drag-and-drop listener
  useEffect(() => {
    let cancelled = false;
    let unlistenDragDrop: UnlistenFn | null = null;

    const setupDragDrop = async () => {
      const appWindow = getCurrentWebviewWindow();
      const unlisten = await appWindow.onDragDropEvent((event) => {
        if (cancelled) return;
        const { type } = event.payload;
        if (type === "enter" || type === "over") {
          setIsDragOver(true);
        } else if (type === "leave") {
          setIsDragOver(false);
        } else if (type === "drop") {
          setIsDragOver(false);
          if (phaseRef.current === "processing") return;
          const paths = event.payload.paths;
          if (paths && paths.length > 0) {
            startProcessing(paths);
          }
        }
      });

      if (cancelled) {
        unlisten();
      } else {
        unlistenDragDrop = unlisten;
      }
    };

    setupDragDrop();

    return () => {
      cancelled = true;
      unlistenDragDrop?.();
    };
  }, [startProcessing]);

  const handlePickFiles = useCallback(async () => {
    if (phase !== "idle") return;
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: "ファーストレタッチする写真を選択",
        filters: [
          {
            name: "画像ファイル (JPEG / PNG)",
            extensions: ["jpg", "jpeg", "png", "JPG", "JPEG", "PNG"],
          },
        ],
      });
      if (selected && Array.isArray(selected) && selected.length > 0) {
        startProcessing(selected);
      } else if (selected && typeof selected === "string") {
        startProcessing([selected]);
      }
    } catch (e) {
      console.error("File dialog error:", e);
    }
  }, [phase, startProcessing]);

  const handlePickFolder = useCallback(
    async (e?: React.MouseEvent) => {
      e?.stopPropagation();
      if (phase !== "idle") return;
      try {
        const selected = await open({
          directory: true,
          multiple: true,
          title: "ファーストレタッチするフォルダを選択",
        });
        if (selected && Array.isArray(selected) && selected.length > 0) {
          startProcessing(selected);
        } else if (selected && typeof selected === "string") {
          startProcessing([selected]);
        }
      } catch (e) {
        console.error("Folder dialog error:", e);
      }
    },
    [phase, startProcessing]
  );

  const openOutputFolder = useCallback(() => {
    if (result?.output_dir) {
      invoke("open_in_folder", { path: result.output_dir });
    }
  }, [result]);

  // ── Render ────────────────────────────────────────────────────────────

  return (
    <div className={`app ${isDragOver ? "drag-over" : ""}`}>
      {/* Header */}
      <header className="app-header">
        <div className="header-inner">
          <HamsterMascot size={62} />
          <div className="header-text">
            <h1>画像ファーストレタッチくん</h1>
            <p className="header-sub">Photo First-Retouch Kun <span className="version">v1.0.0</span></p>
          </div>
        </div>
      </header>

      {/* Main content */}
      <main className="app-main">
        <AnimatePresence mode="wait">

          {/* IDLE — Drop zone */}
          {phase === "idle" && (
            <motion.div
              key="idle"
              className="drop-zone"
              onClick={handlePickFiles}
              initial={{ opacity: 0, scale: 0.97 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.97 }}
              transition={{ duration: 0.2 }}
            >
              <div className="drop-icon">📷</div>
              <p className="drop-title">写真・フォルダをここにドロップ</p>
              <p className="drop-sub">またはクリックしてファイル・フォルダを選択</p>
              <div className="select-buttons">
                <button
                  type="button"
                  className="btn-select"
                  onClick={(e) => {
                    e.stopPropagation();
                    handlePickFiles();
                  }}
                >
                  🖼️ ファイルを選択
                </button>
                <button
                  type="button"
                  className="btn-select"
                  onClick={handlePickFolder}
                >
                  📁 フォルダを選択
                </button>
              </div>
              <div className="drop-badges">
                <span className="format-badge">JPEG</span>
                <span className="format-badge">PNG</span>
              </div>
              <div className="drop-info-grid">
                <div className="drop-info-item">
                  <span className="info-icon">🎨</span>
                  <span>色相・色温度 完全保持</span>
                </div>
                <div className="drop-info-item">
                  <span className="info-icon">☀️</span>
                  <span>明るさ・コントラスト 自動調和</span>
                </div>
                <div className="drop-info-item">
                  <span className="info-icon">📂</span>
                  <span>first_retouched/ に自動保存</span>
                </div>
                <div className="drop-info-item">
                  <span className="info-icon">🔒</span>
                  <span>元データは完全保持（非破壊）</span>
                </div>
              </div>
            </motion.div>
          )}

          {/* PROCESSING */}
          {phase === "processing" && (
            <motion.div
              key="processing"
              className="processing-view"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              <div className="hamster-bounce">
                <HamsterMascot size={80} />
              </div>
              <p className="processing-title">レタッチ処理中…</p>
              {progress && (
                <>
                  <div className="progress-bar-wrap">
                    <div
                      className="progress-bar-fill"
                      style={{ width: `${(progress.current / progress.total) * 100}%` }}
                    />
                  </div>
                  <p className="progress-label">
                    {progress.current} / {progress.total} — <span className="progress-filename">{progress.file_name}</span>
                  </p>
                </>
              )}
            </motion.div>
          )}

          {/* DONE */}
          {phase === "done" && result && (
            <motion.div
              key="done"
              className="done-view"
              initial={{ opacity: 0, y: 16 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
            >
              <div className="done-header">
                <span className="done-icon">✅</span>
                <div>
                  <p className="done-title">処理完了！</p>
                  <p className="done-sub">
                    {result.succeeded} / {result.total} 枚 を調和しました
                  </p>
                </div>
              </div>

              <div className="done-actions">
                <button className="btn-primary" onClick={openOutputFolder}>
                  📂 出力フォルダを開く
                </button>
                <button className="btn-secondary" onClick={reset}>
                  🔄 別の写真を処理
                </button>
              </div>

              <div className="result-output-path">
                <span className="path-label">保存先 ▶ </span>
                <span className="path-value">{result.output_dir}/</span>
              </div>

              <div className="stats-scroll">
                {result.stats.map((stat) => (
                  <StatCard key={stat.file_name} stat={stat} />
                ))}
              </div>
            </motion.div>
          )}

          {/* ERROR */}
          {phase === "error" && (
            <motion.div
              key="error"
              className="error-view"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
            >
              <span className="error-icon">⚠️</span>
              <p className="error-title">エラーが発生しました</p>
              <p className="error-msg">{errorMsg}</p>
              <button className="btn-primary" onClick={reset}>もう一度試す</button>
            </motion.div>
          )}

        </AnimatePresence>
      </main>

      {/* Footer */}
      <footer className="app-footer">
        <span>🐹 ファーストレタッチ: 色相完全保全 · CIE L*a*b* 明度調和 · 適応型CLAHE</span>
      </footer>
    </div>
  );
}

