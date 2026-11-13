# 🐹📷 画像ファーストレタッチくん (Photo First-Retouch Kun)

> **「色味を変えずに、一瞬でくっきり鮮明に。」**  
> Drop your photos. Hue stays. Everything else gets better.

---

## 概要

**画像ファーストレタッチくん** は、JPEG / PNG 画像のファーストレタッチ（一次補正）を自動化するデスクトップアプリ & CLI ツールです。

### 技術的特長

| 項目 | 内容 |
|---|---|
| **色相・色温度** | **完全不変**（CIE L\*a\*b\* の a\*, b\* チャネルはゼロ接触） |
| **明度調和** | 適応型ガンマ（γ ∈ \[0.65, 1.35\]）で露出中央値を標準値に自動整合 |
| **コントラスト** | 適応型 CLAHE（8×8 タイル）＋ ハイライト/シャドウ ロールオフ |
| **Exif 保持** | カメラメタデータ（GPS・機種・露出値）を非破壊継承 |
| **Orientation** | 二重回転問題（Exif タグの誤認識）を根本防止 |
| **出力先** | 元ファイルと同階層に `first_retouched/` を自動作成（元データ保全） |
| **実行環境** | 外部ランタイム不要・単一実行ファイル（Pure Rust） |

---

## インストール

### macOS / Windows — GUI アプリ

[GitHub Releases](https://github.com/YOUR_REPO/image-first-retouch-kun/releases) から最新版をダウンロードしてください。

| プラットフォーム | ファイル |
|---|---|
| macOS (Apple Silicon) | `_x.x.x_aarch64.dmg` |
| macOS (Intel)         | `_x.x.x_x64.dmg`     |
| Windows (NSIS)        | `_x.x.x_x64-setup.exe` |
| Windows (MSI)         | `_x.x.x_x64_ja-JP.msi` |

### CLI — `first-retouch`

```bash
# macOS: アプリバンドル内のバイナリをPATHに追加
export PATH="$PATH:/Applications/画像ファーストレタッチくん.app/Contents/MacOS"

# フォルダを一括処理（first_retouched/ に自動保存）
first-retouch /path/to/photos

# 目標明度 L* を指定（デフォルト: 122.0）
first-retouch /path/to/photos -t 115.0

# 比較HTMLレポートも生成（デフォルト: ON）
first-retouch /path/to/photos --report
```

---

## 使い方（GUI）

1. アプリを起動
2. 写真ファイル（JPEG / PNG）またはフォルダを**ドラッグ＆ドロップ**
3. 自動処理が始まります🐹📷
4. 完了後「**出力フォルダを開く**」をクリックして確認

---

## アルゴリズム詳細

```
Input (JPEG/PNG)
    ↓
1. CIE L*a*b* 変換（sRGB → XYZ → L*a*b*）
    ↓
2. 被写体マスク（L* > 240 の白飛び領域を除外）
    ↓
3. 適応型ガンマ: γ = clip( log(L_target/255) / log(L_med/255), 0.65, 1.35 )
    ↓
4. 適応型 CLAHE（8×8 タイル、clipLimit = 1.2〜2.0）
    ↓
5. ブレンド: L_out = 0.75 × CLAHE + 0.25 × γ_curve
    ↓
6. ロールオフ: L* > 225 → tanh 圧縮 / L* < 12 → sin リフト
    ↓
7. a*, b* チャネルと再合成（色相・色温度は変化ゼロ）
    ↓
8. L*a*b* → XYZ → sRGB 逆変換
    ↓
9. JPEG 保存（Quality 96, 4:4:4）+ Exif APP1 セグメント移植
```

---

## 開発・ビルド

### 必要な環境

- Rust 1.75+
- Node.js 18+
- Tauri CLI v2

```bash
# 依存インストール
npm install

# 開発モードで起動
npm run tauri dev

# リリースビルド（.app / .exe）
npm run tauri build

# CLI バイナリのみビルド
cargo build --release --bin first-retouch --manifest-path src-tauri/Cargo.toml
```

---

## ライセンス

MIT License

---

*「はむはむ画像圧縮くん」の姉妹アプリ 🐹*

