# ToneDock - Agent Guide

Rust 2024 edition / GPL-3.0 / ギター練習用 VST3 ホストアプリ

## Build & Test

```sh
cargo check
cargo test
cargo build --release
```

- `cargo check` は 0 warnings で通すこと
- `cargo test` は通常テストを全通させること
- 実プラグインの editor 検証が必要なときは `TEST_VST_EDITOR_PATH` を指定して ignored smoke test を使う

```sh
$env:TEST_VST_EDITOR_PATH='C:\Program Files\Common Files\VST3\NeuralAmpModeler.vst3'
cargo test smoke_open_plugin_editor -- --ignored --nocapture
```

## Architecture

```text
src/
|- main.rs            eframe エントリポイント
|- app.rs             ToneDockApp (GUI, ViewMode 切替, Undo/Redo, テンプレート)
|- metronome.rs       旧スタンドアローンメトロノーム
|- looper.rs          旧スタンドアローンルーパー
|- session.rs         JSON セッション保存/復元 + 旧フォーマット移行
|- undo.rs            UndoManager
|- audio/
|  |- mod.rs
|  |- chain.rs        旧プラグインチェーン
|  |- engine.rs       cpal オーディオエンジン
|  |- node.rs         NodeId, NodeType, Port, Connection
|  |- graph.rs        AudioGraph DAG
|  `- graph_command.rs UI -> Audio の GraphCommand
|- ui/
|  |- mod.rs
|  |- theme.rs
|  |- controls.rs
|  |- meters.rs
|  |- rack_view.rs
|  |- node_editor.rs
|  `- preferences.rs
`- vst_host/
   |- mod.rs
   |- scanner.rs      VST3 プラグインスキャナー
   |- plugin.rs       VST3 ローダー / processor-controller 初期化 / host objects
   |- editor.rs       VST3 editor hosting
   `- seh_wrapper.c   プラグイン呼び出しの SEH 保護
```

## Key Design Patterns

### Thread Communication (UI -> Audio)

- UI -> Audio は `crossbeam_channel::unbounded()` で `GraphCommand` を送る
- Audio 側はコールバック内で `try_recv()` ループ -> clone -> apply -> `ArcSwap::store()`
- Audio スレッドは `arc_swap.load()` -> `&self::process()` で処理し、グラフ全体のロックを避ける

### AudioGraph Clone & ArcSwap

- `ArcSwap<AudioGraph>` で UI/Audio 間のグラフを共有する
- UI は `load()` -> `clone()` -> mutate -> `store(Arc::new(...))`
- Audio は `load()` -> `Guard` -> `&self::process()`
- `GraphNode::clone()` は `plugin_instance` を `None` にする

### Mono-In / Stereo-Out

- AudioInput は常に 1ch (Mono)
- AudioOutput は常に 2ch (Stereo)
- `gather_inputs()` で Mono -> Stereo 複製、Stereo -> Mono 平均化を行う

## VST3 Host Notes

### Processor / Controller lifecycle

- `IEditController` は `IComponent` から直接取れないケースがある
- split controller の場合は `getControllerClassId()` -> factory から controller 生成 -> `initialize()` が必要
- separate controller を作ったら `IConnectionPoint::connect()` で component/controller を相互接続する
- editor を開く前に component `getState()` -> controller `setComponentState()` を通して state を同期する

### Host objects

- `IHostApplication::createInstance()` は `null` を返し続けてはいけない
- 少なくとも `IMessage` / `IAttributeList` / `IBStream` を返せる必要がある
- iPlug2 系プラグインは UI オープン時に host message を使うことがある

### Windows plugin loading

- Windows の一部 VST3 とくに iPlug2 系は DLL load 後に `InitDll()` が必要
- unload 時は `ExitDll()` も呼ぶ
- これを省くと `gHINSTANCE` 未初期化のまま GUI resource 読み込みに失敗し、`attached()` で落ちることがある

### Editor hosting

- editor attach は UI thread 前提で扱う
- `IPlugView::attached()` の前に `setFrame()` を通す
- close 時は `removed()` の前に `setFrame(nullptr)` を戻す
- Windows では child HWND を用意して attach する
- plugin 呼び出しは `seh_wrapper.c` 経由で保護する

## Important Notes

### Rust 2024 Edition

- 暗黙参照のパターンに注意する
- 複数要素の可変借用には `split_at_mut()` を使う

### Code Style

- コメントは原則追加しない
- `#[allow(dead_code)]` は将来利用予定の public API に限る

### Undo/Redo

- `UndoManager` は `app.rs` 内で管理する
- 連続ドラッグは同一ノードなら自動マージされる
- VST プラグインロードの undo は未対応

### Session Migration

- 旧セッションの `chain` は `migrate_legacy_session()` で `SerializedGraph` に変換する
- `graph` は `#[serde(default)]` で後方互換を維持している
