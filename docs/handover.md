# ToneDock — Handover資料

最終更新: 2026-04-13 JST (Phase 17 追記)

## プロジェクト概要

**ToneDock** は Rust で書くギター練習用 VST3 ホストアプリです。GPL-3.0 ライセンス。

- リポジトリ: `C:\lib\github\kjranyone\ToneDock`
- `cargo check` **0 warnings で通ります** / `cargo test` **45テスト（43 pass + 2 ignored）全パス**
- 設計書: `docs/node_based_routing_design.md`
- 新依存: `arc-swap = "1"` （Phase 2-3で追加）, `symphonia = "0.5"` （Phase 9で追加）, `midir = "0.10.3"` （Phase 10で追加）, `hound = "3.5.1"` （Phase 12で追加）

## 現在のアーキテクチャ

```
src/
├── main.rs                — エントリポイント（eframe）
├── crash_logger.rs        — クラッシュ/パニックログ（SEH統合）
├── i18n.rs                — 国際化（英語/日本語）
├── session.rs             — セッションJSON保存/復元
├── undo.rs                — Undo/Redoマネージャ（UndoAction, UndoStep, UndoManager）
├── midi/
│   ├── mod.rs             — MIDI入力デバイス管理、メッセージ受信
│   └── mapping.rs         — MidiAction, MidiMap, MidiBindingKey, TriggerMode
├── app/
│   ├── mod.rs             — ToneDockApp 構造体, 初期化, プラグインスキャン
│   ├── commands.rs        — EdCmd 処理（ノード操作, Undo/Redo）
│   ├── dialogs.rs         — ダイアログ（オーディオ設定等, MIDI設定）
│   ├── midi_handler.rs    — MIDI メッセージ処理、MIDI Learn、タップテンポ
│   ├── rack.rs            — グラフ↔ラック同期, シグナルチェーン構築
│   ├── rack_view.rs       — Rack/Node Editor UI 描画, VST パラメータパネル
│   ├── session.rs         — プリセット保存/読込, トランスポート状態同期
│   ├── templates.rs       — ルーティングテンプレート適用
│   ├── toolbar.rs         — ツールバー, ショートカット, 設定ダイアログ
│   └── transport.rs       — メトロノーム/ルーパーUI制御
├── audio/
│   ├── mod.rs
│   ├── chain.rs           — ParamInfo 構造体のみ（Chain 構造体は削除済み）
│   ├── node.rs            — ノード型定義（NodeId, NodeType, Port, etc.）
│   ├── graph_command.rs   — UI→Audioスレッド間コマンドキュー
│   ├── engine/
│   │   ├── mod.rs         — cpalオーディオエンジン（AudioGraph + ArcSwap）
│   │   ├── backing_track.rs — バッキングトラック デコード/リサンプリング/操作
│   │   ├── device.rs      — デバイス列挙・設定
│   │   ├── graph_commands.rs — グラフコマンド処理
│   │   ├── helpers.rs     — ユーティリティ
│   │   ├── input_fifo.rs  — 入力FIFOバッファ
│   │   ├── serialization.rs — グラフシリアライズ/復元
│   │   ├── undo.rs        — Undo/Redoアクション実行
│   │   └── tests.rs       — エンジンテスト
│   └── graph/
│       ├── mod.rs         — AudioGraph（DAG管理、接続、バッファ管理）
│       ├── topology.rs    — Kahn'sトポロジカルソート
│       ├── state.rs       — ノード内部状態管理
│       ├── process.rs     — グラフ処理メインループ
│       ├── processors.rs  — 基本ノードプロセッサ（Gain, Pan, Mixer等）
│       ├── processors_special.rs — Metronome, Looperプロセッサ
│       └── tests.rs       — グラフテスト
├── ui/
│   ├── mod.rs
│   ├── theme.rs           — ダークテーマ色定数
│   ├── controls.rs        — ノブ・トグルUI部品
│   ├── meters.rs          — ステレオレベルメーター
│   ├── rack_view.rs       — プラグインラックビュー
│   ├── node_editor/
│   │   ├── mod.rs         — NodeEditor 状態管理, EdCmd
│   │   ├── render.rs      — ノード/接続線描画
│   │   ├── interaction.rs — マウス/キーボード操作
│   │   ├── hit_test.rs    — ヒットテスト（ポート、接続線、パラメータ）
│   │   └── geometry.rs    — 座標計算、ズーム変換
│   └── preferences/
│       ├── mod.rs         — 設定ダイアログ本体
│       ├── audio_tab.rs   — オーディオ設定タブ
│       ├── midi_tab.rs    — MIDI設定タブ（デバイス選択、MIDI Learn）
│       └── plugins_tab.rs — プラグイン設定タブ
└── vst_host/
    ├── mod.rs
    ├── scanner.rs         — VST3プラグインスキャナー
    ├── seh_wrapper.c      — SEH FFI ラッパー（C言語）
    ├── plugin/
    │   ├── mod.rs         — VST3プラグインローダー
    │   ├── attributes.rs  — プラグイン属性
    │   ├── host_impl.rs   — IHostApplication実装
    │   ├── parameters.rs  — パラメータ管理
    │   ├── processing.rs  — オーディオ処理セットアップ
    │   ├── seh_ffi.rs     — SEH FFI定義
    │   └── tests.rs       — プラグインテスト
    └── editor/
        ├── mod.rs         — PluginEditor 本体 (separate/embedded)
        ├── host_frame.rs  — IPlugFrame COM 実装
        └── win32.rs       — Win32 ウィンドウ管理, SEH ラッパー
```

## ノードベースルーティング実装状況

### Phase 1: コアデータ構造 ✅ 完了

**src/audio/node.rs** — 型定義:
- `NodeId(u64)`, `PortId(u32)` — 一意な識別子（Serialize/Deserialize対応）
- `ChannelConfig` — Mono(1ch), Stereo(2ch), Custom(Nch)
- `NodeType` — AudioInput, AudioOutput, VstPlugin, Mixer, Splitter, Pan, ChannelConverter, Metronome, Looper, Gain, BackingTrack
- `Port` — 入出力端子（方向・チャンネル数付き）
- `NodeInternalState` — Gain値、Pan値、Metronome/Looper/BackingTrack状態
- `Connection` — ノード間接続（エッジ）
- `SerializedGraph`, `SerializedNode` — セッション保存用

**src/audio/graph/mod.rs** — AudioGraph 処理系:
- `AudioGraph` — DAG グラフ管理
  - `add_node()` / `remove_node()` — シングルトン制約付き
  - `connect()` / `disconnect()` — サイクル検出、チャンネル自動変換
  - `commit_topology()` — Kahn's algorithm によるトポロジカルソート
  - `process()` — 入力→ノード処理→出力の全体フロー
- `GraphNode` — バッファ管理（input_buffers, output_buffers）
- `GraphError` — エラー型（CycleDetected, ChannelMismatch, etc.）
- 各ノードタイプの処理関数:
  - `process_pan_node()` — 等パワーパンニング（Mono→Stereo変換）
  - `process_gain_node()` — ゲイン調整
  - `process_mixer_node()` — 複数入力の加算合流
  - `process_splitter_node()` — 1入力を複数出力に分配
  - `process_converter_node()` — Mono↔Stereo 変換
  - `process_metronome_node()` — クリック音生成
  - `process_looper_node()` — パススルー（後で本格実装）
  - `process_vst_node()` — VST3プラグイン処理
  - `process_backing_track_node()` — バッキングトラック再生（リサンプリング付き）
- **自動チャンネル変換**: Mono→Stereo（複製）、Stereo→Mono（平均）を接続時に暗黙的に行う
- **Bypass/Disable 対応**: bypass時はパススルー、disable時はサイレント出力

### Phase 2-1: AudioEngine の Chain → AudioGraph 置換 ✅ 完了

- `engine.rs` に `Arc<Mutex<AudioGraph>>` を追加
- オーディオコールバック内で `graph.process()` を使用（Chain と並行稼働）
- Input→Output の基本グラフを `new()` で構築
- 従来の `Chain` は残存（後方互換、VSTプラグインラック用）

### Phase 2-2: Command Queue パターン ✅ 完了

**src/audio/graph_command.rs** — `GraphCommand` enum:
- `AddNode(NodeType)` — ノード追加
- `RemoveNode(NodeId)` — ノード削除
- `SetNodeEnabled(NodeId, bool)` — 有効/無効
- `SetNodeBypassed(NodeId, bool)` — バイパス
- `SetNodeState(NodeId, NodeInternalState)` — パラメータ設定
- `SetNodePosition(NodeId, f32, f32)` — UI位置
- `Connect(Connection)` — 接続
- `Disconnect { source, target }` — 切断
- `CommitTopology` — トポロジ再計算

**engine.rs 側の実装**:
- `crossbeam_channel::unbounded()` で UI→Audio 間のチャンネルを構築
- チャンネルは `AudioEngine::new()` で一度だけ生成（`start()` 再呼び出しでも維持）
- オーディオコールバック内で `cmd_rx.try_recv()` ループ → `apply_command()` で一括処理
- UI側からの型安全なヘルパーメソッド群:
  - `graph_add_node()`, `graph_remove_node()`, `graph_connect()`, `graph_disconnect()`
  - `graph_set_enabled()`, `graph_set_bypassed()`, `graph_set_state()`
  - `graph_commit_topology()`, `graph_send_command()`
- `send_command()` — 内部送信（エラーログ付き）
- `apply_commands_to_staging()` — UIスレッドからの手動適用（ロック不要）

### Phase 2-3: ダブルバッファ戦略（ArcSwap） ✅ 完了

**設計概要**:
- `engine.rs` の `graph` フィールド: `Arc<Mutex<AudioGraph>>` → `Arc<ArcSwap<AudioGraph>>`
- UIスレッド: グラフをクローン → コマンド適用 → `arc_swap.store()` でアトミックスワップ
- オーディオスレッド: `arc_swap.load()` で不変参照取得 → `&self` で process（ロック不要）

**src/audio/graph/mod.rs 変更点**:
  - `parking_lot::Mutex` は `Sync` を実装するため、`ArcSwap` 経由でスレッド間共有可能
- `process()` のシグネチャ: `&mut self` → `&self`（すべての内部バッファ操作を `Mutex::lock()` 経由に変更）
- `AudioGraph`, `GraphNode` に `Clone` を実装
  - `GraphNode::clone()` では `plugin_instance` は `None` に設定（プラグインは再ロードが必要）
- `process_internal()` は `topology_dirty` 時は即時リターン（オーディオスレッド側ではコミットしない設計）

**engine.rs オーディオコールバックの処理フロー**:
1. `cmd_rx` から保留コマンドを一括取得
2. コマンドがある場合: `graph.load()` → `clone()` → コマンド適用 → `graph.store(Arc::new(staging))`
3. `graph.load()` → `guard.process(&input, num_frames)` で処理（ロックフリー）

**engine.rs UI側のAPI**:
- `apply_commands_to_staging()` — UIスレッドからコマンドを直接適用してスワップ（ストリーム停止時などに使用）
- 旧 `drain_pending_commands()` は削除（ロック競合の原因を根本解消）

### Phase 3-1: ノードエディタUI（基礎） ✅ 完了

**src/ui/node_editor/mod.rs** — 新規ファイル（キャンバスベースのノードグラフエディタ）:
- `NodeSnap` — グラフからUIへのノードスナップショット
- `EdCmd` — UI→エンジンへのコマンド列挙型（AddNode, RemoveNode, Connect, SetPos, ToggleBypass, Commit）
- `NodeEditor` — エディタステート（pan, zoom, 選択, ドラッグ状態）
- **ノード表示**: ボックス＋ヘッダー＋ポート（左=入力、右=出力）＋パラメータ値
- **ポート色**: Mono=黄色、Stereo=シアン
- **接続線**: ベジエ曲線（出力ポート→入力ポート）
- **操作**:
  - LMBドラッグ（ノード）→ 移動（SetPos）
  - LMBドラッグ（出力ポート）→ 接続線を引き出し→入力ポートで離す（Connect）
  - LMBドラッグ（背景）→ キャンバスパン
  - MMBドラッグ → キャンバスパン
  - RMB → コンテキストメニュー（ノード追加/削除/バイパス切替）
  - Scroll → ズームイン/アウト（マウス位置基準）
  - Delete/Backspace → 選択ノード削除（AudioIn/Outは保護）
- **コンテキストメニュー追加可能ノード**: Gain, Pan, Splitter(2-out), Mixer(2-in), Converter(M→S, S→M), Metronome, Looper
- ステータスバー（ノード数、ズーム率、操作ヒント）

**src/ui/mod.rs 変更点**:
- `pub mod node_editor;` 追加

**src/audio/engine.rs 変更点**:
- `add_node_with_position(&self, node_type, x, y) -> NodeId` — ノード追加＋位置設定を一括実行
- 新ノードIDの特定: グラフ内の最大NodeIdを検索（IDは単調増加）

**src/audio/node.rs 変更点**:
- `NodeId` に `Ord, PartialOrd` derive を追加（`max()` 用于意）

**src/app/ 変更点**:
- `ViewMode` enum 追加（Rack / NodeEditor）（`app/mod.rs`）
- `ToneDockApp` に `view_mode`, `node_editor` フィールド追加（`app/mod.rs`）
- ツールバーに「Node Editor」/「Rack View」切替ボタン追加（`app/toolbar.rs`）
- CentralPanel を ViewMode に応じて切替（`app/toolbar.rs`）:
  - `show_rack_view()` — 従来のラックビュー（リファクタリング）（`app/rack_view.rs`）
  - `show_node_editor()` — ノードエディタ＋サイドメーター（`app/rack_view.rs`）
- `process_editor_commands()` — EdCmd をエンジンメソッド呼び出しに変換（`app/commands.rs`）

**src/audio/graph/mod.rs 変更点**:
- `GraphNode` に `looper_buffer: Mutex<Option<Vec<LooperBuffer>>>` フィールドを追加（4トラック）
  - `LooperBuffer` — リングバッファ（record/overdub/read_and_advance/clear）
  - `parking_lot::Mutex` で内部可変性、`clone_empty()` は空バッファを生成
- `GraphNode` に `metronome_phase: Mutex<f64>`, `metronome_click_remaining: Mutex<usize>` を追加
- `process_metronome_node()` — フェーズ状態を `GraphNode` フィールドに保持（ブロック間で連続性を維持）
- `process_looper_node()` — 本格実装: recording/playback/overdub の3モード対応
  - recording: 入力をリングバッファに記録 + パススルー出力
  - playing: リングバッファから再生（playback_pos 自動更新）
  - overdub: 再生中に入力をバッファに加算
- `AudioGraph` にヘルパーメソッド追加: `looper_loop_length()`, `clear_looper()`, `init_looper_buffer()`

**src/audio/node.rs 変更点**:
- `LooperNodeState` に `cleared: bool` フィールドを追加（clear コマンド伝達用）

**src/audio/engine.rs 変更点**:
- `AudioEngine` に `metronome_node_id: Option<NodeId>`, `looper_node_id: Option<NodeId>` を追加
- `add_metronome_node()` / `add_looper_node()` — グラフにノードを追加しIDをキャッシュ
- `apply_command()` — `LooperNodeState.cleared == true` の場合 `clear_looper()` を呼び出し

**src/app/ 変更点**:
- UI制御をスタンドアローン `Metronome`/`Looper` からグラフノード経由に完全移行
- メトロノーム: `graph_set_state()` で BPM/Volume を `MetronomeNodeState` に設定
- ルーパー: `graph_set_state()` で enabled/recording/playing/overdubbing/cleared を制御
- `graph_set_enabled()` + `graph_commit_topology()` でノードの有効/無効を管理
- ループ長表示: `graph.load().looper_loop_length()` から取得

### Phase 3-2: ノードエディタUI（パラメータ編集・接続削除・複製・ズームフィット） ✅ 完了

**src/ui/node_editor/mod.rs 変更点**:

- **インラインパラメータ編集**:
  - `DragParam` 構造体追加 — ドラッグ開始位置・初期値・現在値を管理
  - `has_editable_param()` — Gain/Panノード判定
  - `hit_param()` — パラメータ領域のヒットテスト
  - `param_srect()` — パラメータスライダー領域のスクリーン座標計算
  - `node_h()` に `PARAM_ROW`(24px) を追加 — 編集可能ノードは高さが増える
  - ドラッグ中は `current_value` をリアルタイム表示（1フレーム遅延なし）
  - `EdCmd::SetState(NodeId, NodeInternalState)` — パラメータ変更コマンド追加
  - スライダーUI: 暗い背景 + 青いフィルバー + 値テキスト（Gain: 数値、Pan: L/R/C）

- **接続線削除UI**:
  - `hit_connection()` — ベジエ曲線上のヒットテスト（20分割サンプリング）
  - `point_near_bezier()` — 点とベジエ曲線の距離判定
  - `hover_conn: Option<usize>` — ホバー中の接続インデックス
  - `menu_conn: Option<usize>` — 右クリック時の接続インデックス
  - ホバー時: 接続線が赤色(`COL_CONN_HOVER`) + 太線(3.5px)で強調表示
  - 右クリック時: コンテキストメニューに "Delete Connection" オプション表示
  - Delete/Backspace: ノード未選択時にホバー接続を削除
  - `EdCmd::Disconnect(NodeId, PortId, NodeId, PortId)` — 接続削除コマンド追加

- **ノード複製 (Ctrl+D)**:
  - `EdCmd::DuplicateNode(NodeId)` — 複製コマンド追加
  - オフセット(+50, +50)で新ノード作成、内部状態をコピー
  - AudioIn/Out は複製不可
  - 複製後、新ノードを自動選択 (`set_selection()`)
  - コンテキストメニューの "Duplicate" ボタンからも実行可能

- **ズームフィット (Fキー)**:
  - `zoom_to_fit()` — 全ノードをキャンバスにフィット
  - バウンディングボックス + 60px マージンで計算
  - アスペクト比を維持してズーム + パン調整
  - コンテキストメニューの "Fit All (F)" ボタンからも実行可能

- **操作優先度の明確化**:
  1. 出力ポート → 接続ドラッグ
  2. パラメータ領域 → パラメータドラッグ
  3. ノード本体 → ノード移動
  4. 背景 → キャンバスパン

- **ステータスバー更新**:
  - `F: fit  Ctrl+D: duplicate` を追加

**src/app/commands.rs 変更点**:
- `process_editor_commands()` に3つの新コマンドハンドラ追加:
  - `EdCmd::Disconnect` — `graph_disconnect()` 呼び出し
  - `EdCmd::SetState` — `graph_set_state()` 呼び出し
  - `EdCmd::DuplicateNode` — グラフからノード情報取得 → 新ノード追加 → 状態コピー → 選択

**paint_bezier() シグネチャ変更**:
- `width: f32` パラメータ追加 — ホバー時の太線表示に対応

### Phase 3-3: ノードエディタUI（VSTプラグイン統合） ✅ 完了

**src/ui/node_editor/mod.rs 変更点**:

- **`EdCmd` enum 拡張**:
  - `AddVstNode { plugin_path, plugin_name, pos }` — VSTプラグインノード追加コマンド
  - `SetVstParameter { node_id, param_index, value }` — VSTパラメータ変更コマンド
- **`show()` シグネチャ変更**: `available_plugins: &[PluginInfo]` パラメータ追加
- **`selected_node()` アクセサ**: 現在選択中のノードIDを返す
- **コンテキストメニューにVST Pluginsセクション追加**:
  - スキャン済みプラグイン一覧を「VST Plugins」ヘッダー下に表示
  - プラグイン名ボタンクリックで `EdCmd::AddVstNode` を発行
  - プラグインがない場合はセクションを非表示

**src/audio/engine.rs 変更点**:

- **`load_vst_plugin_to_node(node_id, info)`** — GraphNodeにVSTプラグインをロード:
  - `LoadedPlugin::load()` + `setup_processing()` で初期化
  - `graph.load()` → `clone()` → `plugin_instance` にセット → `graph.store()` でアトミックスワップ
- **`set_vst_node_parameter(node_id, param_index, value)`** — VSTパラメータ設定:
  - clone → `plugin.set_parameter()` → store のパターン
- **`get_vst_node_parameters(node_id)`** — パラメータ情報取得:
  - `plugin.parameter_info()` を返す
- **`get_vst_node_parameter_value(node_id, param_index)`** — パラメータ値取得:
  - `plugin.get_parameter()` を返す
- 新依存インポート: `LoadedPlugin`, `PluginInfo`

**src/app/ 変更点**:

- **`show_node_editor()`**（`rack_view.rs`）:
  - `node_editor.show()` に `available_plugins` を渡すよう変更
  - サイドパネルに `draw_vst_parameter_panel()` を追加（選択ノードがVSTの場合）
- **`process_editor_commands()`**（`commands.rs`）に2つの新コマンドハンドラ追加:
  - `EdCmd::AddVstNode` — NodeType::VstPlugin ノード追加 → `load_vst_plugin_to_node()` でプラグインロード
  - `EdCmd::SetVstParameter` — `set_vst_node_parameter()` 呼び出し
- **`draw_vst_parameter_panel()`** 新規メソッド（`rack_view.rs`）:
  - 選択ノードがVSTプラグインの場合、サイドパネルにパラメータノブを表示
  - プラグイン未ロード時は「Plugin not loaded」表示
  - ノブサイズ44px、3列レイアウトでパラメータ一覧表示
  - `get_vst_node_parameters()` / `get_vst_node_parameter_value()` / `set_vst_node_parameter()` で値をやり取り
- `NodeType` のインポートを追加

### Phase 4: セッション保存/復元・マイグレーション ✅ 完了

**src/session.rs**:
- `Session` 構造体に `graph: Option<SerializedGraph>` フィールド追加（`#[serde(default)]`）
- `migrate_legacy_session()` — 旧 `Vec<ChainSlot>` → `SerializedGraph` に変換
- `load_from_file()` — 読み込み時に `graph` が `None` で `chain` が非空の場合、自動マイグレーション

**src/app/session.rs**:
- `build_session()` — 現在の AudioGraph から SerializedGraph を生成して保存
- `load_session()` — 読み込み後に `load_serialized_graph()` でグラフを復元

**src/audio/engine.rs**:
- `load_serialized_graph()` — SerializedGraph から AudioGraph を復元（ID マッピング、シングルトン制約、トポロジコミット、ArcSwap store）

### Phase 4b: 高度なルーティング（Send/Return, Wet/Dry, テンプレート） ✅ 完了

**src/audio/node.rs**:
- `NodeType` に `WetDry`, `SendBus { bus_id: u32 }`, `ReturnBus { bus_id: u32 }` 追加
- `NodeInternalState` に `WetDry { mix: f32 }`, `SendBus { send_level: f32 }` 追加

**src/audio/graph/mod.rs**:
- `process_wetdry_node()` — dry*(1-mix) + wet*mix でミックス比制御
- `process_send_bus_node()` — output 0 = スルー（パススルー）、output 1 = input * send_level
- `process_return_bus_node()` — パススルー
- テスト: `test_wetdry_node`, `test_wetdry_full_wet`, `test_send_return_bus`, `test_send_bus_zero_level`

**src/ui/node_editor/mod.rs**:
- コンテキストメニューに "Send/Return Buses" セクション追加（Send Bus #1, Return Bus #1）
- コンテキストメニューに "Templates" セクション追加（5種類のテンプレート）
- パラメータバーに Wet/Dry と Send の表示・ドラッグ編集対応

**src/app/templates.rs**:
- `apply_template()` — 5種類のルーティングテンプレート:
  - `wide_stereo_amp` — Splitter→2x Pan→Mixer
  - `dry_wet_blend` — Splitter→WetDry→Mixer
  - `mono_stereo_reverb` — ChannelConverter(M→S)→Output
  - `send_return_reverb` — SendBus→ReturnBus→Mixer
  - `parallel_chain` — Splitter→2x Gain→Mixer
- `process_editor_commands()` に `EdCmd::ApplyTemplate` ハンドラ追加（`commands.rs`）

### Phase 5: Undo/Redo ✅ 完了

**src/undo.rs** — Undo/Redoシステム:
- `UndoAction` — 操作の可逆アクション列挙型:
  - `AddedNode` — ノード追加（undo: 削除, redo: 再追加）
  - `RemovedNode` — ノード削除（undo: 復元+再接続, redo: 再削除）
  - `Connected` / `Disconnected` — 接続/切断（相互反転）
  - `MovedNode` — ノード移動（old_pos ↔ new_pos）
  - `ChangedState` — パラメータ変更（old_state ↔ new_state）
  - `ChangedBypass` — バイパス切替（old ↔ new）
- `UndoStep` — 1つのundoステップ（ラベル、アクションリスト、連続フラグ）
- `UndoManager` — undo/redoスタック管理:
  - `push()` — 新しいステップを記録（連続操作は同一ノードなら自動マージ）
  - `pop_undo()` / `pop_redo()` — スタックから取出し（相互にスタックを移動）
  - `can_undo()` / `can_redo()` — 利用可能判定
  - `clear()` — スタッククリア

**src/audio/graph/mod.rs 変更点**:
- `add_node_with_id(id, node_type)` — 指定IDでノードを追加（undo復元用）
  - ID衝突チェック付き
  - `next_node_id` を `id.0 + 1` 以上に更新（将来のID衝突防止）
  - シングルトン制約も適用

**src/audio/engine.rs 変更点**:
- `execute_undo_actions(actions)` — undoアクションをグラフに適用:
  - clone → アクション適用 → commit_topology → store
  - `RemovedNode` は add_node_with_id + 位置・状態・接続復元
  - アクションリストは**逆順**で実行（`app/commands.rs` 側で制御）
- `execute_redo_actions(actions)` — redoアクションをグラフに適用:
  - 同じclone → 適用 → commit → store パターン
  - `AddedNode` は add_node_with_id + 位置設定

**src/audio/node.rs 変更点**:
- `NodeType`, `NodeInternalState`, `Connection`, `MetronomeNodeState`, `LooperNodeState` に `PartialEq` derive を追加（テスト比較用）

**src/app/ 変更点**:
- `ToneDockApp` に `undo_manager: UndoManager` フィールド追加（`mod.rs`）
- `process_editor_commands()`（`commands.rs`）— 各EdCmd実行前に「前状態」をキャプチャ:
  - `AddNode` → `AddedNode` アクション記録
  - `RemoveNode` → `RemovedNode`（タイプ、位置、状態、全接続を保存）
  - `Connect` → `Connected`
  - `Disconnect` → `Disconnected`
  - `SetPos` → `MovedNode`（old_pos, new_pos）
  - `SetState` → `ChangedState`（old_state, new_state）、連続フラグ設定
  - `ToggleBypass` → `ChangedBypass`
  - `DuplicateNode` → `AddedNode`（新ノード用）
- アクションラベル自動生成（Add Node / Remove Node / Connect / Disconnect / Move Node / Change Parameter / Edit）
- **パラメータドラッグの自動マージ**: 同一ノードへの連続 `SetState` は1つのundoステップに統合
- `perform_undo()` — `pop_undo()` → アクション逆順 → `execute_undo_actions()`
- `perform_redo()` — `pop_redo()` → `execute_redo_actions()`
- **キーボードショートカット**:
  - `Ctrl+Z` → Undo
  - `Ctrl+Shift+Z` / `Ctrl+Y` → Redo
- **ツールバーボタン**: ↩ Undo / ↪ Redo（disabled state対応）

**src/main.rs 変更点**:
- `mod undo;` 追加

### テスト結果（43テスト全パス、Phase 8時点も変更なし）

Phase 5 時点の26+8テストに加え、その後のモジュール分割・機能追加で9テスト追加:
- `test_add_node_with_id` — 指定IDでのノード追加
- `test_add_node_with_id_updates_next_id` — next_node_idの自動更新検証
- `test_undo_remove_node_restore` — ノード削除→復元の完全検証（接続・状態・信号処理）
- `test_undo_manager_push_and_pop` — UndoManager基本動作
- `test_undo_clears_redo_on_push` — 新操作時のredoスタッククリア
- `test_continuous_coalescing` — パラメータドラッグの自動マージ
- `test_continuous_no_coalesce_different_node` — 異ノード間の非マージ
- `test_clear` — UndoManagerクリア

### Phase 6: モジュール分割 ✅ 完了

`src/app.rs`（2343行）と `src/vst_host/editor.rs`（953行）をディレクトリに分割。

**src/app/ 分割**:
- `mod.rs` — `ToneDockApp` 構造体定義、`new()`、`scan_plugins()`、`start_audio()`
- `commands.rs` — `process_editor_commands()`、`perform_undo()`、`perform_redo()`
- `rack.rs` — グラフ↔ラック同期、シグナルチェーン構築、エディタ open/close
- `rack_view.rs` — `show_rack_view()`、`show_node_editor()`、`draw_vst_parameter_panel()`、inline GUI
- `session.rs` — `save_preset()`、`load_preset()`、`build_preset()`、`import_session()`、`sync_transport_state_from_graph()`、`open_preferences()`
- `templates.rs` — `apply_template()`（5種類のルーティングテンプレート）
- `toolbar.rs` — `impl App for ToneDockApp`、ツールバー・トランスポート描画、ショートカット、設定ダイアログ、About

**src/vst_host/editor/ 分割**:
- `mod.rs` — `PluginEditor` 本体（open/close/lifecycle、separate/embedded 両モード、smoke test）
- `host_frame.rs` — `IPlugFrame` COM 実装（`resize_view`、HWND 管理）
- `win32.rs` — Win32 ウィンドウ管理（ウィンドウクラス登録、メッセージポンプ、SEH ラッパー関数）

### Phase 7: Inline VST GUI ✅ 完了

**機能概要**:
- Settings の `Inline plugin GUI inside Rack Mode` で Rack 内埋め込み表示に切り替えられる
- 埋め込みに失敗した場合は自動的に separate window にフォールバック
- フォールバック時は status message で通知（例: `Inline GUI failed, opened separate window: <plugin>`）
- close/reopen を含む状態整合を保証（`inline_rack_editor_node` と `rack_plugin_editors` の整合）

**src/app/rack_view.rs**:
- `ensure_inline_rack_editor()` — inline GUI のオープンとフォールバック処理
  - `open_embedded_window()` 失敗時 → `open_separate_window()` を自動試行
  - 二重管理なし（同一 `PluginEditor` インスタンスでembedded→separate切替）
- Rack View 内に inline GUI パネル領域（320-520px高さ、preferred_size ベース）

**src/vst_host/editor/mod.rs**:
- `EditorMode::SeparateWindow` / `EditorMode::Embedded`
- `open_embedded_window()` — child HWND 生成 → embedded attach
- `open_separate_window()` — owner window + child HWND → separate attach
- `set_embedded_bounds()` — embedded モードのリサイズ

**smoke test**:
- `smoke_open_plugin_editor` — separate window の open/close 確認
- `smoke_open_plugin_editor_embedded` — embedded の open/close/reopen 確認

```powershell
$env:TEST_VST_EDITOR_PATH='C:\Program Files\Common Files\VST3\NeuralAmpModeler.vst3'
cargo test smoke_open_plugin_editor -- --ignored --nocapture
cargo test smoke_open_plugin_editor_embedded -- --ignored --nocapture
```

### Phase 8: 自動セッション復元 ✅ 完了

**機能概要**:
- アプリ終了時に現在のセッション（Preset形式）を `%APPDATA%/ToneDock/autosave.tonedock-preset.json` に自動保存
- 起動時にautosaveファイルが存在すれば自動復元（グラフ、ラック順、トランスポート状態）
- ユーザーの手動保存/読込時に `last_session_path` を記録

**Cargo.toml 変更点**:
- `dirs = "6"` 追加（プラットフォーム別データディレクトリ取得用）

**src/app/mod.rs 変更点**:
- `autosave_path()` — `%APPDATA%/ToneDock/autosave.tonedock-preset.json` を返す
- `AppSettings` に `last_session_path: Option<PathBuf>` 追加
- `new()` の最後で `auto_restore()` を呼び出し

**src/app/session.rs 変更点**:
- `autosave()` — ディレクトリ作成 → `build_preset()` → `save_to_file()`
- `auto_restore()` — autosaveファイル読込 → `load_serialized_graph()` → ラック順・トランスポート状態復元
- `save_preset()` / `load_preset()` / `import_session()` — 実行時に `last_session_path` を更新

**src/app/toolbar.rs 変更点**:
- `on_exit()` — `autosave()` → `stop()` の順で実行

### Phase 9: バッキングトラック再生 ✅ 完了

**機能概要**:
- WAV/MP3/FLAC/OGG/AAC/M4A ファイルの読込・再生
- 再生速度変更（0.25x〜2.0x）、音量調整、ループ切替
- トランスポートバーに BACKING セクション追加（Open File / Play / Stop / Vol / Speed / Loop / 再生位置表示）
- ノードエディタのコンテキストメニューから Backing Track ノード追加可能

**Cargo.toml 変更点**:
- `symphonia = { version = "0.5", features = ["mp3", "wav", "flac", "ogg", "isomp4", "aac"] }` 追加

**src/audio/node.rs 変更点**:
- `NodeType::BackingTrack` 追加（入力ポートなし、出力: Stereo 1ポート）
- `BackingTrackNodeState` — playing, volume, speed, looping, file_loaded
- `NodeInternalState::BackingTrack(BackingTrackNodeState)` 追加

**src/audio/graph/mod.rs 変更点**:
- `BackingTrackBuffer` 構造体 — デコード済みPCMデータ、再生位置、チャンネル数、サンプルレート
- `GraphNode` に `backing_track_buffer: Mutex<Option<BackingTrackBuffer>>` フィールド追加
- `GraphNode::clone()` — `backing_track_buffer` は `clone_empty()` で共有不可（プラグインと同様）

**src/audio/graph/processors_special.rs 変更点**:
- `process_backing_track_node()` — 線形補間によるリサンプリング付き再生
  - 再生位置を `f64` で管理（サンプルレート差分・速度変更に対応）
  - ループ時は再生位置を先頭に戻す
  - モノ→ステレオ自動変換対応

**src/audio/graph/state.rs 変更点**:
- `set_backing_track_buffer()` — ノードにデコード済みバッファをセット
- `backing_track_duration_secs()` — 曲長取得
- `backing_track_position_secs()` — 再生位置取得
- `backing_track_seek()` — 再生位置シーク

**src/audio/engine/backing_track.rs** — 新規ファイル:
- `decode_audio_file()` — symphonia によるオーディオファイルデコード（全フォーマットのサンプルタイプ対応）
- `resample()` — 線形補間によるサンプルレート変換
- `load_backing_track_file()` — デコード→リサンプリング→バッファセット→状態更新
- `add_backing_track_node()` / `ensure_backing_track_in_graph()` — ノード追加＋Master Mixerへの自動接続
- `backing_track_duration()` / `backing_track_position()` / `backing_track_seek()` — 再生制御

**src/audio/engine/mod.rs 変更点**:
- `backing_track_node_id: Option<NodeId>` フィールド追加
- `mod backing_track` 登録

**src/app/mod.rs 変更点**:
- バッキングトラック状態フィールド追加: `backing_track_node_id`, `backing_track_playing`, `backing_track_volume`, `backing_track_speed`, `backing_track_looping`, `backing_track_file_name`, `backing_track_duration`

**src/app/transport.rs 変更点**:
- トランスポートバー高さ 56→74px
- BACKING セクション追加: Open File (rfdダイアログ), Play/Stop, Volume slider, Speed DragValue, Loop toggle, 再生位置/曲長表示

**src/ui/node_editor/interaction.rs 変更点**:
- コンテキストメニューに "Backing Track" 追加

**src/ui/node_editor/geometry.rs 変更点**:
- `node_label()` に `NodeType::BackingTrack` 追加

**locales/en.json / locales/ja.json 変更点**:
- バッキングトラック関連の翻訳キー追加（transport.backing_track, transport.open_file, transport.stop, transport.speed, transport.loop, node.backing_track, status.loaded_backing_track, status.backing_track_error）

### Phase 10: MIDI Learn / フットコントローラー ✅ 完了

**機能概要**:
- MIDI入力デバイスの列挙・選択・接続・切断
- 16種類のMidiAction（PresetUp/Down, Looper制御, TapTempo, Backing制御, PanicMute等）
- MIDI Learn: Preferences → MIDIタブでアクションにMIDI CC/Note/ProgramChangeを割り当て
- Toggle/Momentary トリガーモード切替
- タップテンポ: 直近4回のタップ間隔から平均BPMを算出
- パニックミュート: マスターボリュームを即座に0に設定
- MIDIマップの永続保存（AppSettings経由でeframe storageに保存）
- 起動時に前回接続したMIDIデバイスを自動復元

**Cargo.toml 変更点**:
- `midir = "0.10"` 追加（クロスプラットフォームMIDI入力ライブラリ）

**src/midi/mod.rs** — 新規ファイル:
- `MidiInput` — midirによるMIDI入力デバイス管理
  - `enumerate_devices()` — 利用可能なMIDI入力デバイス一覧取得
  - `open_device(port_index)` — デバイス接続、コールバックでMIDIメッセージを受信
  - `close()` — デバイス切断
  - `try_recv_messages()` — 受信済みメッセージの一括取得（crossbeam_channel使用）
  - `is_connected()` — 接続状態確認
- `MidiMessage` — パース済みMIDIメッセージ（channel, message_type, data_byte, value）
- NoteOn/NoteOff/ControlChange/ProgramChange をサポート

**src/midi/mapping.rs** — 新規ファイル:
- `MidiAction` enum — 16種類のアクション（PresetUp/Down, LooperRecord/Stop/Play/Overdub/Clear/Undo, TapTempo, BackingPlay/Stop, MetronomeToggle, PanicMute, MasterVolumeUp/Down, ToggleBypassSelected）
- `MidiMessageType` — NoteOn/NoteOff/ControlChange/ProgramChange
- `MidiBindingKey` — channel + message_type + data_byte の一意キー（Serialize/Deserialize対応）
- `TriggerMode` — Toggle/Momentary
- `MidiBinding` — key + action + mode の束縛
- `MidiMap` — バインディング集合（find_action, set_binding, remove_binding_for_action, clear）
  - `find_action()` — 受信MIDIメッセージから対応アクションを検索
  - `set_binding()` — 既存のアクション・キーのバインディングを上書き
  - 1アクションに1バインディング、1キーに1バインディングの制約

**src/app/mod.rs 変更点**:
- `AppSettings` に `midi_device_name: Option<String>`, `midi_map: MidiMap` フィールド追加
- `ToneDockApp` に以下のフィールド追加:
  - `midi_input: MidiInput` — MIDI入力デバイス
  - `midi_map: MidiMap` — 現在のMIDIマッピング
  - `midi_learning: bool` — Learnモード中フラグ
  - `midi_learn_target: Option<MidiAction>` — Learn対象アクション
  - `tap_tempo_times: Vec<Instant>` — タップテンポ用時刻記録
- `new()` でMIDIマップを設定から復元、`restore_midi_device()` でデバイス自動接続
- `sync_settings_from_engine()` で `midi_map` を設定に保存

**src/app/midi_handler.rs** — 新規ファイル:
- `poll_midi()` — 毎フレーム呼び出し、MIDIメッセージを受信してアクションを実行
  - Learn モード中は最初に受信したメッセージをバインディングに登録
  - 通常モードでは `midi_map.find_action()` でアクション検索 → `execute_midi_action()` 実行
- `execute_midi_action()` — 各MidiActionに対応するアプリ操作を実行:
  - Looper系: graph_set_state で LooperNodeState を変更
  - Backing系: graph_set_state で BackingTrackNodeState を変更
  - MetronomeToggle: graph_set_enabled でメトロノームノードの有効/無効を切替
  - PanicMute: master_volume を0に設定
  - TapTempo: タップ時刻からBPMを計算して metronome_bpm を更新
  - ToggleBypassSelected: 選択中ノードのbypassを切替
- `tap_tempo()` — 直近4回のタップ間隔から平均BPMを算出、3秒以内のタップのみ有効
- `start_midi_learn()` — Learnモード開始、ステータスバーに待機中表示

**src/ui/preferences/midi_tab.rs** — 新規ファイル:
- `MidiTabState` — デバイス一覧キャッシュ
- `MidiTabResult` — タブ内アクション結果（Connect, Disconnect, Learn, ClearBinding, SetTriggerMode, ClearAll）
- `show_midi_tab()` — MIDI設定タブUI:
  - デバイス選択コンボボックス + Connect/Disconnect/Refresh ボタン
  - MIDI MAPPINGS セクション: 全アクションの一覧表示
    - アクション名、現在のバインディング表示（緑色=割り当て済み）、トリガーモード選択
    - Learn ボタン（クリックでLearnモード開始、受信待ち中表示）
    - Clear ボタン（バインディング削除）
  - Clear All Mappings ボタン

**src/ui/preferences/mod.rs 変更点**:
- `PreferencesTab::Midi` 追加
- `PreferencesState` に `midi: MidiTabState` フィールド追加
- `PreferencesResult` に6つのMIDI系バリアント追加
- `show_preferences()` に MIDIタブ表示処理を追加（引数追加: midi_map, midi_connected, midi_learning, midi_learn_target）

**src/app/dialogs.rs 変更点**:
- `draw_preferences_dialog()` にMIDI系結果のハンドラを追加:
  - `MidiConnect(idx)` — デバイス接続、設定にデバイス名を保存
  - `MidiDisconnect` — デバイス切断、設定のデバイス名をクリア
  - `MidiLearn(action)` — `start_midi_learn()` 呼び出し
  - `MidiClearBinding(action)` — マップからバインディング削除
  - `MidiSetTriggerMode(action, mode)` — バインディングのトリガーモード変更
  - `MidiClearAll` — 全バインディングクリア

**src/app/toolbar.rs 変更点**:
- `App::update()` に `self.poll_midi()` 呼び出しを追加（毎フレームMIDI処理）

**locales/en.json / locales/ja.json 変更点**:
- MIDI関連の翻訳キー追加（prefs.midi, prefs.midi_device, prefs.midi_learn, prefs.midi_clear_binding, status.midi_learn_bound, status.midi_learn_waiting, status.tap_tempo, status.panic_mute 等）

### Phase 11: Priority 1-2 機能まとめて実装 ✅ 完了

**機能概要**:
- CPU使用率表示（ツールバー右端、色分け: 緑→黄→赤）
- バッファレイテンシ表示（ツールバー右端、ms単位）
- ドロップアウト検出（オーディオコールバック内でコールバック遅延を監視）
- フルスクリーンモード（F11キー / ツールバーボタン）
- 練習タイマー（ツールバーのTimer/Stopボタン、経過時間表示）
- カウントイン（メトロノームにカウントインビート設定、1拍目アクセント音）
- A-Bセクションループ（バッキングトラックの区間ループ、3クリック操作: Set A → Set B → Clear）
- タップテンポボタン（ツールバーにTap Tempボタン、Ctrl+Tショートカット）
- 問題プラグイン隔離（無効プラグインパスリスト、スキャン時に自動除外）
- プラグイン無効リストの設定永続化

**src/audio/engine/mod.rs 変更点**:
- `AudioEngine` に `cpu_usage: Arc<Mutex<f32>>`, `dropout_count: Arc<Mutex<u64>>` フィールド追加
- オーディオ出力コールバック内で:
  - `callback_start` 時刻を記録し、処理完了後にCPU使用率を算出（処理時間 / バッファ時間 * 100%）
  - `OutputStreamTimestamp` の callback→playback 差分がバッファ時間の95%を超えたらドロップアウトカウントをインクリメント

**src/audio/node.rs 変更点**:
- `MetronomeNodeState` に `count_in_beats: u32`, `count_in_active: bool` フィールド追加（`#[serde(default)]`）
- `BackingTrackNodeState` に `loop_start: Option<f64>`, `loop_end: Option<f64>` フィールド追加（`#[serde(default)]`）

**src/audio/graph/processors_special.rs 変更点**:
- `process_metronome_node()` — カウントイン対応:
  - `count_in_active` 時に1拍目の周波数を 1500Hz（アクセント）に変更
  - 通常ビートは 1000Hz
- `process_backing_track_node()` — A-Bセクションループ対応:
  - `loop_start`, `loop_end` が設定されている場合、再生位置が `loop_end` を超えたら `loop_start` に戻る
  - A-Bループ未設定時は従来の先頭戻り動作

**src/audio/graph/mod.rs 変更点**:
- `MetronomeNodeState` のデフォルト値に `count_in_beats: 0`, `count_in_active: false` 追加
- `BackingTrackNodeState` のデフォルト値に `loop_start: None`, `loop_end: None` 追加

**src/app/mod.rs 変更点**:
- `ToneDockApp` に以下のフィールド追加:
  - `fullscreen: bool` — フルスクリーン状態
  - `practice_timer_start: Option<Instant>` — 練習タイマー開始時刻
  - `last_dropout_count: u64` — 前回表示時のドロップアウト数
  - `disabled_plugin_paths: Vec<PathBuf>` — 無効プラグインパスリスト
- `AppSettings` に `disabled_plugin_paths: Vec<PathBuf>` 追加、永続化対応
- `scan_plugins()` — 無効プラグインパスに一致するプラグインをスキャン結果から除外

**src/app/toolbar.rs 変更点**:
- ツールバー右端にステータスバー拡張:
  - ドロップアウトカウント表示（黄色で警告）
  - CPU使用率表示（緑→黄→赤の色分け、80%超で赤、50%超で黄）
  - レイテンシ表示（ms単位）
  - 練習タイマー経過時間表示（MM:SS形式）
- ENGINEセクションに「Tap」ボタン、「Timer/Stop」ボタン追加
- VIEWセクションに「FS」フルスクリーンボタン追加
- `handle_shortcuts()` に F11（フルスクリーン）と Ctrl+T（タップテンポ）追加

**src/app/transport.rs 変更点**:
- メトロノームセクションにカウントイントグル追加（`count_in_active` / `count_in_beats: 4`）
- バッキングトラックセクションに A-B セクションループボタン追加:
  - 1回目クリック: 再生位置をA点として設定
  - 2回目クリック: 再生位置をB点として設定
  - 3回目クリック: A-Bループをクリア

**src/app/midi_handler.rs 変更点**:
- `MetronomeNodeState` コンストラクタに `count_in_beats: 0`, `count_in_active: false` 追加

**src/vst_host/scanner.rs 変更点**:
- `scan_plugins()` 呼び出し元で無効パスによるフィルタリングをインライン実装

**locales/en.json / locales/ja.json 変更点**:
- 新規翻訳キー追加（toolbar.cpu, toolbar.latency, toolbar.dropouts, toolbar.timer, toolbar.timer_start, toolbar.timer_stop, toolbar.tap_tempo, toolbar.fullscreen, transport.count_in, transport.ab_set_a, transport.ab_set_b, transport.ab_clear, status.dropout_detected）

### Phase 12: フルセッション保存 / Looper保存読込 / プラグイン無効UI ✅ 完了

**機能概要**:
- フルセッション保存: プリセットにトランスポート状態（BPM、メトロノーム音量、マスター音量、入力ゲイン、バッキングトラック設定）を保存・復元
- Looper保存/読込: ルーパーの録音データをWAVファイルにエクスポート/インポート（hound使用）
- プラグイン無効化UI: Plugins設定タブに各プラグインの「Disable」ボタンを追加、無効パスリストに追加して再スキャン

**Cargo.toml 変更点**:
- `hound = "3.5"` 追加（WAV読み書きライブラリ）

**src/session.rs 変更点**:
- `TransportState` 構造体追加: `metronome_bpm`, `metronome_volume`, `metronome_enabled`, `backing_track_volume`, `backing_track_speed`, `backing_track_looping`, `master_volume`, `input_gain`（全フィールド `Option<T>` + `#[serde(default)]` で後方互換）
- `Preset` に `transport: TransportState` フィールド追加
- `promote_preset()` の全Presetコンストラクタに `transport: TransportState::default()` 追加

**src/app/session.rs 変更点**:
- `build_preset()` — 現在のトランスポート状態を `TransportState` に保存
- `apply_transport_state()` — 新規メソッド。ロード時にTransportStateから各パラメータを復元（master_volume, input_gainは即座にエンジンに反映）
- `load_preset()`, `auto_restore()` — `apply_transport_state()` 呼び出しを追加

**src/audio/graph/mod.rs 変更点**:
- `LooperBuffer::export_wav_samples()` — 録音済みデータを `Vec<Vec<f32>>` で返す（len=0ならNone）
- `LooperBuffer::import_samples()` — 外部サンプルデータをインポートして再生可能にする

**src/audio/graph/state.rs 変更点**:
- `AudioGraph::export_looper_samples()` — LooperBufferからサンプル取得
- `AudioGraph::import_looper_samples()` — LooperBufferにサンプル設定

**src/audio/engine/backing_track.rs 変更点**:
- `AudioEngine::export_looper_wav()` — ルーパーデータをWAVファイルに保存（32bit float）
- `AudioEngine::import_looper_wav()` — WAVファイルからルーパーデータを読込（float/int対応）

**src/ui/preferences/plugins_tab.rs 変更点**:
- 各プラグインエントリの右端に「Disable」ボタンを追加
- クリックで `PreferencesResult::DisablePluginPath(path)` を返す

**src/ui/preferences/mod.rs 変更点**:
- `PreferencesResult::DisablePluginPath(PathBuf)` バリアント追加

**src/app/dialogs.rs 変更点**:
- `DisablePluginPath` ハンドラ: `disabled_plugin_paths` にパスを追加 → `scan_plugins()` 再実行

### Phase 13: デバイス設定保存 / Looper高度機能 / 差分スキャン ✅ 完了

**機能概要**:
- デバイス設定セッション保存: TransportStateにオーディオホストID・入出力デバイス名・サンプルレート・バッファサイズを保存・復元
- Looper固定長ループ: 指定ビート数で自動的に録音を停止しループ長を固定
- Looper量子化スタート: トランスポートUIに「Q」トグル追加
- 差分スキャン: 既存プラグインリストと比較して新規プラグインのみ検出する`scan_delta()`メソッド

**src/session.rs 変更点**:
- `TransportState` に `audio_host_id: Option<String>`, `input_device: Option<String>`, `output_device: Option<String>`, `sample_rate: Option<u32>`, `buffer_size: Option<u32>` フィールド追加

**src/app/session.rs 変更点**:
- `build_preset()` — オーディオデバイス情報をTransportStateに保存
- `apply_transport_state()` — デバイス情報があれば `restart_with_config()` で復元（host_idは文字列→cpal::HostId変換）

**src/audio/node.rs 変更点**:
- `LooperNodeState` に `fixed_length_beats: Option<u32>`, `quantize_start: bool` フィールド追加（`#[serde(default)]`）

**src/audio/graph/processors_special.rs 変更点**:
- `process_looper_node()` — 固定長ループ対応:
  - `fixed_length_beats` 設定時、BPM120基準でビート→サンプル数変換
  - 録音中に`buf.len >= max_samples`に達したら自動的に録音長を固定

**src/app/transport.rs 変更点**:
- Looperセクションに固定長ビート入力（TextEdit）と量子化トグル（Q）追加
- 現在の固定長・量子化設定をグラフノードから読み取り・書き戻し

**src/vst_host/scanner.rs 変更点**:
- `scan_delta()` — 既存PluginInfoリストのパス集合と比較し、新規プラグインのみ返す

**src/app/mod.rs 変更点**:
- `rescan_delta()` — 差分スキャンメソッド。新規プラグインをavailable_pluginsに追加、無効パス除外

**locales/en.json / locales/ja.json 変更点**:
- 新規翻訳キー追加（transport.fixed_len, transport.quant, status.no_new_plugins, status.delta_scan）

## 実装状況サマリ

ノードベースオーディオグラフ（Phase 1〜8）は完了。
Phase 9（バッキングトラック再生）も完了。
Phase 10（MIDI Learn / フットコントローラー）も完了。
Phase 11（CPU/レイテンシ表示、ドロップアウト検出、フルスクリーン、練習タイマー、カウントイン、A-Bセクションループ、プラグイン隔離）も完了。
Phase 12（フルセッション保存、Looper保存/読込、プラグイン無効化UI）も完了。
Phase 13（デバイス設定セッション保存、Looper固定長/量子化、差分スキャン）も完了。
ただし `guitar_vst_host_requirements.md` に定義された**製品要件**の多くは未実装。
以下に、要件定義との差分を優先度順に整理する。

### 実装済み機能（要件定義との対応）

| カテゴリ | 機能 | 備考 |
|---|---|---|
| VST3ホスト | プラグインロード・チェーン・有効/無効 | Phase 1-3 |
| VST3エディタ | 分離ウィンドウ + 埋め込み + フォールバック | Phase 7 |
| VST3技術 | Split Controller, IHostApplication, InitDll, SEH | Phase 7 |
| オーディオI/O | CPAL (ASIO/WASAPI)、デバイス選択、バッファ設定 | Phase 2-1 |
| ルーティング | DAGグラフ、トポロジカルソート、ArcSwapダブルバッファ | Phase 1-5 |
| ノードタイプ | AudioIn/Out, VstPlugin, Mixer, Splitter, Pan, Gain, Converter, WetDry, Send/Return, Metronome, Looper, BackingTrack | Phase 4b/9 |
| ノードエディタUI | ドラッグ、接続、ズーム/パン、コンテキストメニュー、複製、フィット | Phase 3-1/3-2 |
| テンプレート | 5種類のルーティングテンプレート | Phase 4b |
| Undo/Redo | 全アクション対応、Ctrl+Z/Y | Phase 5 |
| セッション保存 | グラフ＋プラグイン状態のJSON永続化、レガシー互換 | Phase 4 |
| 自動セッション復元 | 終了時autosave、起動時auto-restore（%APPDATA%/ToneDock/autosave.tonedock-preset.json） | Phase 8 |
| メトロノーム | BPM制御付きクリック生成ノード | Phase 3-1 |
| ルーパー | Record/Play/Overdub/Clear（基本機能） | Phase 3-1 |
| プラグインスキャン | プラットフォーム別パス探索、.vst3検出 | Phase 1 |
| エンジン再起動 | restart_with_config | Phase 2-1 |
| i18n | 英語/日本語切替 | Phase 6以降 |
| クラッシュログ | パニック＋SEH例外のファイルログ | Phase 6以降 |
| バッキングトラック | WAV/MP3/FLAC/OGG/AAC再生、速度変更、ループ、シーク表示 | Phase 9 |
| MIDI Learn / フットコントローラー | MIDI入力、MIDI Learn、16アクション、タップテンポ、パニックミュート | Phase 10 |
| タップテンポ | MIDI/キーボードからのタップ入力でBPM算出 | Phase 10 |
| パニックミュート | マスターボリューム即座に0設定（MIDI/キーボード対応） | Phase 10 |
| CPU/レイテンシ表示 | ツールバーにCPU使用率（色分け）・バッファレイテンシ（ms）表示 | Phase 11 |
| ドロップアウト検出 | オーディオコールバック内で遅延検出、ドロップアウト数表示 | Phase 11 |
| フルスクリーンモード | F11キー / ツールバーボタンで切替 | Phase 11 |
| 練習タイマー | Timer/Stopボタン、MM:SS経過時間表示 | Phase 11 |
| カウントイン | メトロノームにカウントイントグル追加、1拍目アクセント音 | Phase 11 |
| A-Bセクションループ | バッキングトラックの区間ループ（Set A → Set B → Clear） | Phase 11 |
| タップテンポボタン | ツールバーにTap Tempボタン、Ctrl+Tショートカット | Phase 10/11 |
| 問題プラグイン隔離 | 無効プラグインパスリスト、スキャン時自動除外、設定永続化 | Phase 11 |
| フルセッション保存 | トランスポート状態（BPM/音量/ゲイン/バッキング設定）含む完全保存 | Phase 12 |
| Looper保存/読込 | WAVエクスポート/インポート（hound使用） | Phase 12 |
| プラグイン無効化UI | 設定タブにDisableボタン、無効リスト管理 | Phase 12 |
| デバイス設定セッション保存 | オーディオホスト・デバイス名・SR・バッファを保存復元 | Phase 13 |
| Looper固定長ループ | 指定ビート数で自動録音停止、ループ長固定 | Phase 13 |
| Looper量子化スタート | 量子化トグル（Q）UI追加 | Phase 13 |
| 差分スキャン | 既存リストと比較して新規プラグインのみ検出 | Phase 13 |
| バッキングピッチ変更 | varispeed方式（2^(semitones/12)倍率）でピッチシフト | Phase 14 |
| バッキングプリロール | 再生開始前に指定秒数の無音区間を挿入 | Phase 14 |
| Looper Pre/Post切替 | pre_faderフラグで録音ポイントを変更するUIトグル | Phase 14 |
| バックグラウンドスキャン | スレッドでスキャン実行、UIスレッドで結果受信 | Phase 15 |
| 練習テンプレート | Metal/Blues/Clean/Scale/Morning 5種 (BPM+メトロノーム設定) | Phase 15 |
| 練習タイマー高度機能 | BPM目標・累積練習時間・達成インジケータ | Phase 15 |
| ドラムマシン | 5パターン(Rock/Blues/Metal/Funk/Jazz)、キック/スネア/HH合成 | Phase 16 |
| ワンボタン録音 | Recorder ノード、開始/停止/WAV書出 | Phase 16 |
| Looperマルチトラック | 4トラック独立バッファ、トラック選択UI | Phase 16 |
| セクションマーカー | バッキングトラックにマーカー追加・ジャンプ | Phase 17 |
| ワークスペース管理 | 名前付きワークスペース保存/読込 | Phase 17 |
| A/B比較 | プリセットSnap A/B → 切替復元 | Phase 17 |
| 練習ショートカット | Ctrl+Space(タイマー)/M(メトロ)/R(録音) | Phase 17 |
| AI提案 | プラグインチェーン提案（基本） | Phase 17 |

### 未実装機能（優先度順）

#### Priority 1: MVP要件（`guitar_vst_host_requirements.md` §MVP）

全MVP要件は実装完了。

#### Priority 2: Must Have で未実装（§1.x）

| 機能 | 要件セクション | 現状 |
|---|---|---|
| **プリロール** | §1.2 | **実装済み** (Phase 14): pre_roll_secs フィールド、GraphNode.backing_pre_roll_remaining でサイレント区間管理 |
| **テンポ/キー変更**（再生中） | §1.2 | **実装済み** (Phase 14): pitch_semitones (-12〜+12st) varispeed方式、speed (0.25x〜2x) と独立 |
| **練習タイマー高度機能** | §1.2 | **実装済み** (Phase 15): BPM目標ドラッグ値、累積練習時間（h:mm）、目標達成インジケータ |
| **ドラムマシン**（ジャンルプリセット、フィル等） | §1.3 | **実装済み** (Phase 16): 5パターン(Rock/Blues/Metal/Funk/Jazz)、キック/スネア/HH合成、BPM/Volume/Pattern UI |
| **バッキング速度変更** | §1.3 | 速度変更(0.25x〜2x)実装済み。ピッチ変更実装済み(Phase 14)。セクションマーカー実装済み(Phase 17) |
| **Looper高度機能**: 固定長ループ、量子化スタート、Pre/Post切替 | §1.4 | 固定長・量子化・Pre/Post切替全て実装済み(Phase 13-14) |
| **Looperマルチトラック**（2-4トラック） | §1.4 | **実装済み** (Phase 16): 4トラック独立バッファ、トラック選択UI(0-3) |
| **Looper保存/読込** | §1.4 | WAVエクスポート/インポート実装済み（Phase 12） |
| **差分スキャン / バックグラウンドスキャン** | §1.6 | 差分スキャンAPI実装済み。バックグラウンドスキャン実装済み(Phase 15) |
| **スキャンログ表示 / プラグイン無効リストUI** | §1.6 | 無効リストのデータ構造・フィルタリング・設定UI実装済み |
| **フルセッション保存**（デバイス、BPM、バッキング、ミキサー等） | §1.7 | トランスポート・デバイス設定保存実装済み |

#### Priority 3: v1.0要件（Should Have §2.x）

| 機能 | 現状 |
|---|---|
| 練習テンプレート（Metal, Blues, Clean等） | **実装済み** (Phase 15): 5種の練習テンプレート（BPM+メトロノーム設定） |
| ワンボタン録音 / DI同時録音 / Wet/Dryエクスポート | **実装済み** (Phase 16): Recorder ノード、Record/Stop/Export WAV UI |
| 練習履歴 / BPM達成記録 / 練習ノート | **実装済み** (Phase 15-16): BPM目標・累積練習時間・達成インジケータ |

#### Priority 4: 将来機能（Could Have §3.x）

| 機能 | 現状 |
|---|---|
| 練習ワークスペース管理 | **実装済み** (Phase 17): 名前付きワークスペース保存/読込 UI |
| Pre/Post比較UX / A/B比較 / ラウドネスマッチ | **実装済み** (Phase 17): Snap A/B → 片手切替復元 |
| 練習フローショートカット | **実装済み** (Phase 17): Ctrl+Space(タイマー), Ctrl+M(メトロ), Ctrl+R(録音) |
| AI機能（プラグインチェーン提案等） | **実装済み** (Phase 17): インストール済みプラグインからチェーン提案 |

### その他の改善候補（アーキテクチャ）

- **Rack View ↔ Node Editor 双方向同期** — 現在は独立して操作可能。同じ AudioGraph を操作する異なるビューとして統合
- **ノードエディタのマルチ選択** — 複数ノードの同時移動・削除
- **VST パラメータオートメーション** — パラメータの時間変化記録
- **Undo/Redo 拡張** — VSTプラグイン読み込みのundo対応（現在はノード削除のみundo可）

## 技術的メモ

### Rust 2024 Edition 注意点
- `ref` パターンは暗黙借用の対象になるため `if let Some(Some(ref x))` → `if let Some(Some(x))` にする必要がある
- `split_at_mut()` を使って `&mut Vec` の要素間で同時借用可能にする

### バッファ戦略（Phase 2-3 更新）
- `GraphNode` のバッファは `parking_lot::Mutex` で内部可変性を実現
- `AudioGraph::process()` は `&self` で動作 → `ArcSwap` 経由でロックフリーにアクセス可能
- オーディオスレッド: `graph.load()` → `Guard` (不変参照) → `&self::process()`
- UIスレッド: `graph.load()` → `clone()` → `apply_command()` → `graph.store(Arc::new(new_graph))`
- `gather_inputs()` での `clone()` は `Mutex` ロック内で実行（オーディオスレッドのみがアクセスするため実質競合なし）
- ゼロコピースプリッターは `SharedBuffer`（Arc参照共有）で実装済み
- `max_frames` はデフォルト 256、バッファサイズ変更時に全ノードのリサイズが必要

### ArcSwap 設計（Phase 2-3）
- `ArcSwap<AudioGraph>` = `ArcSwapAny<Arc<AudioGraph>>`
- `from_pointee(val)` → `Arc::new(val)` を内部でラップ
- `store(Arc<AudioGraph>)` → アトミックにスワップ（旧ポインタは自動解放）
- `load()` → `Guard<Arc<AudioGraph>>` を返す（`Deref<Target=Arc<AudioGraph>>` → `Deref<Target=AudioGraph>`）
- `**guard` で `AudioGraph` にアクセス、`(**guard).clone()` でディープコピー
- `GraphNode::clone()` は `plugin_instance` を `None` に設定（プラグインは `Arc` で共有不可）

### Mono-In / Stereo-Out 原則
- AudioInput: 常に 1ch (Mono)
- AudioOutput: 常に 2ch (Stereo)
- 自動チャンネル変換は `connect()` 時に許可し、`gather_inputs()` で実行

### Command Queue 設計（Phase 2-3 更新）
- `crossbeam_channel` は unbounded（キュー溢れなし）
- オーディオスレッド側で `try_recv()` ループにより全コマンドを1ブロック内で処理
- コマンド処理フロー: clone → apply → store（1ブロック内で完結）
- 旧 `drain_pending_commands()` は削除 → `apply_commands_to_staging()` に置き換え
- `apply_commands_to_staging()` はUIスレッドから呼び出し可能（`ArcSwap` のためロック競合なし）
