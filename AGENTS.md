# ToneDock - Agent Guide

Rust 2024 edition / GPL-3.0 / ギター練習用 VST3 ホストアプリ

## Build & Test

```sh
cargo check
cargo test
cargo build --release
```

- `cargo check` は 0 warnings で通すこと
- `cargo test` は通常テストを全通させること（43テスト + 2 ignored）
- 実プラグインの editor 検証が必要なときは `TEST_VST_EDITOR_PATH` を指定して ignored smoke test を使う

```sh
$env:TEST_VST_EDITOR_PATH='C:\Program Files\Common Files\VST3\NeuralAmpModeler.vst3'
cargo test smoke_open_plugin_editor -- --ignored --nocapture
```

## Architecture

```text
src/
|- main.rs            eframe エントリポイント
|- crash_logger.rs    クラッシュ/パニックログ (SEH統合)
|- i18n.rs            国際化 (英語/日本語)
|- session.rs         JSON セッション保存/復元
|- undo.rs            UndoManager
|- app/
|  |- mod.rs          ToneDockApp 構造体, 初期化, プラグインスキャン
|  |- commands.rs     EdCmd 処理 (ノード操作, Undo/Redo)
|  |- dialogs.rs      ダイアログ (オーディオ設定, MIDI設定)
|  |- midi_handler.rs MIDI メッセージ処理, MIDI Learn, タップテンポ
|  |- rack.rs         グラフ↔ラック同期, シグナルチェーン構築
|  |- rack_view.rs    Rack/Node Editor UI 描画, VST パラメータパネル
|  |- session.rs      プリセット保存/読込, トランスポート状態同期
|  |- templates.rs    ルーティングテンプレート適用
|  |- toolbar.rs      ツールバー, ショートカット, 設定ダイアログ
|  `- transport.rs    メトロノーム/ルーパー/バッキングトラックUI制御
|- midi/
|  |- mod.rs          MIDI入力デバイス管理, メッセージ受信 (midir)
|  `- mapping.rs      MidiAction, MidiMap, MidiBindingKey, TriggerMode
|- audio/
|  |- mod.rs
|  |- chain.rs        ParamInfo struct (旧プラグインチェーン削除済み)
|  |- node.rs         NodeId, NodeType, Port, Connection
|  |- graph_command.rs UI -> Audio の GraphCommand
|  |- engine/
|  |  |- mod.rs       cpal オーディオエンジン (AudioGraph + ArcSwap)
|  |  |- backing_track.rs バッキングトラック デコード/リサンプリング/操作
|  |  |- device.rs    デバイス列挙・設定
|  |  |- graph_commands.rs グラフコマンド処理
|  |  |- helpers.rs   ユーティリティ
|  |  |- input_fifo.rs 入力FIFOバッファ
|  |  |- serialization.rs グラフシリアライズ/復元
|  |  |- undo.rs      Undo/Redoアクション実行
|  |  `- tests.rs     エンジンテスト
|  `- graph/
|     |- mod.rs       AudioGraph (DAG管理, 接続, バッファ管理)
|     |- topology.rs  Kahn'sトポロジカルソート
|     |- state.rs     ノード内部状態管理
|     |- process.rs   グラフ処理メインループ
|     |- processors.rs 基本ノードプロセッサ (Gain, Pan, Mixer等)
|     |- processors_special.rs Metronome, Looper, BackingTrack, DrumMachine, Recorderプロセッサ
|     `- tests.rs     グラフテスト
|- ui/
|  |- mod.rs
|  |- theme.rs        ダークテーマ色定数
|  |- controls.rs     ノブ・トグルUI部品
|  |- meters.rs       ステレオレベルメーター
|  |- rack_view.rs    プラグインラックビュー
|  |- node_editor/
|  |  |- mod.rs       NodeEditor 状態管理, EdCmd
|  |  |- render.rs    ノード/接続線描画
|  |  |- interaction.rs マウス/キーボード操作
|  |  |- hit_test.rs  ヒットテスト (ポート, 接続線, パラメータ)
|  |  `- geometry.rs  座標計算, ズーム変換
|  `- preferences/
|     |- mod.rs       設定ダイアログ本体
|     |- audio_tab.rs オーディオ設定タブ
|     |- midi_tab.rs  MIDI設定タブ (デバイス選択, MIDI Learn)
|     `- plugins_tab.rs プラグイン設定タブ
`- vst_host/
   |- mod.rs
   |- scanner.rs      VST3 プラグインスキャナー
   |- plugin/
   |  |- mod.rs       VST3 ローダー / processor-controller 初期化 / host objects
   |  |- attributes.rs プラグイン属性
   |  |- host_impl.rs IHostApplication実装
   |  |- parameters.rs パラメータ管理
   |  |- processing.rs オーディオ処理セットアップ
   |  |- seh_ffi.rs   SEH FFI定義
   |  `- tests.rs     プラグインテスト
   `- editor/
      |- mod.rs       PluginEditor 本体 (separate/embedded)
      |- host_frame.rs IPlugFrame COM 実装 (resize_view)
      `- win32.rs     Win32 ウィンドウ管理, SEH ラッパー
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

### MIDI Input & Learn

- `midir` で MIDI デバイスを列挙・接続
- 受信メッセージは `crossbeam_channel::bounded(256)` でUIスレッドに転送
- `poll_midi()` を毎フレーム `App::update()` 内で呼び出し
- Learn モード: `midi_learning = true` + `midi_learn_target = Some(action)` の間、最初に受信したメッセージをバインディングに登録
- `MidiMap` は `AppSettings` 経由で eframe storage に永続化（起動時に復元）
- `TriggerMode::Toggle` / `Momentary` はバインディングごとに設定可能

### CPU / Dropout / Timer / Fullscreen (Phase 11)

- `AudioEngine.cpu_usage` はオーディオコールバック内で処理時間/バッファ時間から算出
- `AudioEngine.dropout_count` はコールバック遅延がバッファ95%超時インクリメント
- 練習タイマーは `Instant` ベースのシンプルなストップウォッチ
- フルスクリーンは `ViewportCommand::Fullscreen` で制御（F11 / ツールバーボタン）
- 無効プラグインパスリストは `AppSettings.disabled_plugin_paths` に保存、`scan_plugins()` で除外
- A-Bセクションループは `BackingTrackNodeState.loop_start/loop_end` で制御
- カウントインは `MetronomeNodeState.count_in_beats/count_in_active` で制御

### Session & Looper (Phase 12)

- `Preset` に `TransportState` を含む（BPM、音量、ゲイン等を保存・復元）
- `TransportState` の全フィールドは `Option<T>` + `#[serde(default)]` で後方互換
- `LooperBuffer` のエクスポート/インポートAPIは `AudioEngine::export_looper_wav/import_looper_wav`（hound使用）
- プラグイン無効化は `disabled_plugin_paths` にパスを追加して `scan_plugins()` で除外
- `LooperNodeState` に `fixed_length_beats: Option<u32>` と `quantize_start: bool` で固定長・量子化を制御
- 差分スキャンは `PluginScanner::scan_delta()` で既存リストとの差分のみ検出

### Backing Track Pitch & Pre-Roll / Looper Pre-Fader (Phase 14)

- `BackingTrackNodeState.pitch_semitones` (-12〜+12st) は varispeed 方式でピッチシフト: `ratio *= 2^(semitones/12)`
- `BackingTrackNodeState.pre_roll_secs` は再生開始前のサイレント区間、`GraphNode.backing_pre_roll_remaining` でフレームカウントダウン
- `LooperNodeState.pre_fader` は looper の録音ポイント（Pre/Post）切替フラグ
- `ToneDockApp` に `backing_track_pitch_semitones`, `backing_track_pre_roll_secs`, `looper_pre_fader` フィールド追加
- `TransportState` にも `backing_track_pitch_semitones`, `backing_track_pre_roll_secs`, `looper_pre_fader` を追加（`Option<T>` + `#[serde(default)]`）
- transport UI に Pitch ドラッグ値 (-12st〜+12st) と Pre-Roll ドラッグ値 (0s〜10s) と Pre トグルを追加

### Background Scan & Practice Templates & Timer Advanced (Phase 15)

- プラグインスキャンは `std::thread::spawn` でバックグラウンド実行、`crossbeam_channel::bounded(1)` で結果受信
- `ScanResult` enum（`Full`/`Delta`）でフルスキャンと差分スキャンを区別
- `poll_scan_results()` を毎フレーム `update()` 内で呼び出し、スキャン中はスピナー表示
- `Chain` 構造体は使用されなくなったため削除、`ParamInfo` のみ `chain.rs` に残す
- 練習テンプレート: `apply_practice_template()` で BPM・メトロノーム・カウントインを一括設定
- 5種: Metal(160), Blues(80), Clean(100), Scale(90), Morning(120)
- BPM目標: `AppSettings.bpm_goal: Option<f64>` でドラッグ値設定、現在BPMが目標に達したら "Goal!" 表示
- 累積練習時間: `AppSettings.total_practice_secs: u64` にタイマー停止時に加算、ステータスバーに "Total: XhXXm" 表示

### Drum Machine / Recorder / Looper Multi-track (Phase 16)

- `DrumMachine` ノードタイプ: 5パターン (Rock/Blues/Metal/Funk/Jazz)、16ステップ
- サウンド合成: キック(周波数スイープ+エンベロープ)、スネア(トーン+ノイズ)、HH(ノイズ)
- `DrumMachineNodeState`: bpm, volume, playing, pattern, current_step
- トランスポートUIにドラムマシンセクション (トグル/BPM/Volume/Pattern)
- `Recorder` ノードタイプ: ステレオ入力をバッファにキャプチャ、WAVエクスポート
- `RecorderNodeState`: recording, has_data
- ツールバーにRECORDERセクション (Add/Record/Stop/Export)
- `GraphNode.recorder_buffer: Mutex<Option<Vec<Vec<f32>>>>` で録音データ保持
- `GraphNode.drum_phase`, `GraphNode.drum_step` でドラムマシンの位相管理
- Looperマルチトラック: `looper_buffer` を `Vec<LooperBuffer>` (4トラック) に変更
- `LooperNodeState.active_track: u8` (0-3) でアクティブトラック選択
- トランスポートUIにトラックセレクタ (Trk 0-3) を追加

### Section Markers / Workspace / A-B / Shortcuts / AI (Phase 17)

- バッキングトラック セクションマーカー: `BackingTrackNodeState.section_markers: Vec<f64>` で位置リスト管理
- トランスポートUIに「+Marker」ボタンとマーカージャンプボタン表示
- ワークスペース管理: `save_workspace()` / `load_workspace()` で名前付きセッション保存/読込
- A/B比較: `snapshot_to_ab('a'|'b')` でプリセットをJSON文字列にスナップ、`restore_ab()` で即時復元
- `preset_a: Option<String>` / `preset_b: Option<String>` で2スロット保持
- 練習フローショートカット: Ctrl+Space(タイマー), Ctrl+M(メトロノーム), Ctrl+R(録音)
- AI提案: `suggest_plugin_chain()` でインストール済みプラグインからチェーン構成提案
- ツールバーに AI Suggest ドロップダウン表示

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

- `UndoManager` は `app/mod.rs` 内で管理する
- 連続ドラッグは同一ノードなら自動マージされる
- VST プラグインロードの undo は未対応

### Session Migration

- 旧セッションの `chain` は `migrate_legacy_session()` で `SerializedGraph` に変換する
- `graph` は `#[serde(default)]` で後方互換を維持している
