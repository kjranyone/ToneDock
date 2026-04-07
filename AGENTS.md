# ToneDock — Agent Guide

Rust 2024 edition / GPL-3.0 / ギター練習用 VST3 ホストアプリ

## Build & Test

```sh
cargo check          # 0 warnings で通ること
cargo test           # 34テスト全パス
cargo build --release
```

## Architecture

```
src/
├── main.rs           — eframe エントリポイント
├── app.rs            — ToneDockApp (GUI, ViewMode 切替, Undo/Redo, テンプレート)
├── metronome.rs      — スタンドアローンメトロノーム (旧, グラフノード経由に移行済み)
├── looper.rs         — スタンドアローンルーパー   (旧, グラフノード経由に移行済み)
├── session.rs        — JSON セッション保存/復元 + 旧フォーマットマイグレーション
├── undo.rs           — UndoManager (UndoAction, UndoStep, 連続ドラッグ自動マージ)
├── audio/
│   ├── mod.rs
│   ├── chain.rs      — 旧プラグインチェーン (後方互換)
│   ├── engine.rs     — cpal オーディオエンジン (ArcSwap<AudioGraph> + command queue)
│   ├── node.rs       — NodeId, NodeType, Port, Connection, NodeInternalState
│   ├── graph.rs      — AudioGraph DAG (トポロジカルソート, &self プロセス, Mutex 内部可変性)
│   └── graph_command.rs — GraphCommand enum (UI→Audio スレッド間)
├── ui/
│   ├── mod.rs
│   ├── theme.rs      — ダークテーマ色定数
│   ├── controls.rs   — ノブ・トグルUI部品
│   ├── meters.rs     — ステレオレベルメーター
│   ├── rack_view.rs  — プラグインラックビュー
│   ├── node_editor.rs — ノードグラフエディタ (キャンバス, ベジエ接続線, パラメータ編集)
│   └── preferences.rs — 設定ダイアログ
└── vst_host/
    ├── mod.rs
    ├── scanner.rs    — VST3 プラグインスキャナー
    └── plugin.rs     — VST3 COM 経由プラグインローダー
```

## Key Design Patterns

### Thread Communication (UI → Audio)

- UI→Audio: `crossbeam_channel::unbounded()` で `GraphCommand` を送信
- Audio側: コールバック内で `try_recv()` ループ → clone → apply → `ArcSwap::store()`
- **ロックフリー処理**: Audio スレッドは `arc_swap.load()` → `&self::process()` (Mutex 不要)

### AudioGraph Clone & ArcSwap

- `ArcSwap<AudioGraph>` で UI/Audio 間のグラフ共有
- UI: `load()` → `clone()` → mutate → `store(Arc::new(...))`
- Audio: `load()` → `Guard` (不変参照) → `&self::process()`
- `GraphNode` のバッファは `parking_lot::Mutex` で内部可変性
- `GraphNode::clone()` は `plugin_instance` を `None` に (プラグインは共有不可)

### Mono-In / Stereo-Out

- AudioInput: 常に 1ch (Mono)
- AudioOutput: 常に 2ch (Stereo)
- 自動チャンネル変換: Mono→Stereo (複製), Stereo→Mono (平均) を `gather_inputs()` で実行

## Important Notes

### Rust 2024 Edition

- `ref` パターンは暗黙借用対象 → `if let Some(Some(ref x))` ではなく `if let Some(Some(x))` を使う
- `split_at_mut()` で `&mut Vec` 要素間の同時借用を可能にする

### Code Style

- コメントは原則として追加しない (ユーザー指示時のみ)
- `#[allow(dead_code)]` は将来使用予定の public API に付与済み

### Undo/Redo

- `UndoManager` は `app.rs` 内で管理
- パラメータドラッグの連続操作は同一ノードなら自動マージ (`continuous` フラグ)
- VST プラグインロードの undo は未対応 (ノード削除のみ)

### Session Migration

- 旧セッション (`chain` フィールド) は `migrate_legacy_session()` で自動的に `SerializedGraph` に変換
- `graph` フィールドは `#[serde(default)]` で後方互換
