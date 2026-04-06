# ToneDock — Handover資料

最終更新: 2026-04-06 17:07 JST

## プロジェクト概要

**ToneDock** は Rust で書くギター練習用 VST3 ホストアプリです。GPL-3.0 ライセンス。

- リポジトリ: `C:\lib\github\kjranyone\ToneDock`
- `cargo check` **0 warnings で通ります** / `cargo test` **22テスト全パス**
- 設計書: `docs/node_based_routing_design.md`
- 新依存: `arc-swap = "1"` （Phase 2-3で追加）## 現在のアーキテクチャ

```
src/
├── main.rs              — エントリポイント（eframe）
├── app.rs               — メインGUIアプリ
├── metronome.rs          — メトロノーム生成（スタンドアローン）
├── looper.rs             — ルーパー（スタンドアローン）
├── session.rs            — セッションJSON保存/復元
├── audio/
│   ├── mod.rs
│   ├── chain.rs          — 従来のプラグインチェーン（後方互換用）
│   ├── engine.rs         — cpalオーディオエンジン（AudioGraph + Chain併用）
│   ├── node.rs           — ノード型定義（NodeId, NodeType, Port, etc.）
│   ├── graph.rs          — AudioGraph（DAG処理、トポロジカルソート、バッファ管理）
│   │                       [Phase 2-3] Mutex内部可変性で&selfプロセス対応、Clone実装
│   └── graph_command.rs  — UI→Audioスレッド間コマンドキュー
├── ui/
│   ├── mod.rs
│   ├── theme.rs          — ダークテーマ色定数
│   ├── controls.rs       — ノブ・トグルUI部品
│   ├── meters.rs         — ステレオレベルメーター
│   ├── rack_view.rs       — プラグインラックビュー
│   ├── node_editor.rs     — ノードグラフエディタ（Phase 3-1）
│   └── preferences.rs    — 設定ダイアログ
└── vst_host/
    ├── mod.rs
    ├── scanner.rs          — VST3プラグインスキャナー
    └── plugin.rs           — VST3 COM経由プラグインローダー
```

## ノードベースルーティング実装状況

### Phase 1: コアデータ構造 ✅ 完了

**src/audio/node.rs** — 型定義:
- `NodeId(u64)`, `PortId(u32)` — 一意な識別子（Serialize/Deserialize対応）
- `ChannelConfig` — Mono(1ch), Stereo(2ch), Custom(Nch)
- `NodeType` — AudioInput, AudioOutput, VstPlugin, Mixer, Splitter, Pan, ChannelConverter, Metronome, Looper, Gain
- `Port` — 入出力端子（方向・チャンネル数付き）
- `NodeInternalState` — Gain値、Pan値、Metronome/Looper状態
- `Connection` — ノード間接続（エッジ）
- `SerializedGraph`, `SerializedNode` — セッション保存用

**src/audio/graph.rs** — AudioGraph 処理系:
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

**src/audio/graph.rs 変更点**:
- `GraphNode` のバッファフィールド（`input_buffers`, `output_buffers`, `plugin_instance`）を `parking_lot::Mutex` でラップ
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

### テスト結果（22テスト全パス）
- `test_add_audio_input_output` — シングルトンノードの追加
- `test_singleton_violation` — 二重追加の拒否
- `test_connect_and_topology` — トポロジカルソート順序の検証
- `test_cycle_detection` — サイクル検出
- `test_remove_node_cleans_connections` — ノード削除時のコネクション整理
- `test_process_simple_chain` — Input→Output の信号処理
- `test_process_with_gain` — Gain ノードの信号処理
- `test_pan_node` — Pan ノードのL/R配分
- `test_splitter_mixer_parallel` — Splitter→2xGain→Mixer のパラレル処理
- `test_mono_stereo_auto_conversion_allowed` — 自動チャンネル変換の許可
- `test_disconnect` — コネクション削除
- `test_bypass_node` — バイパス時のパススルー
- `test_disabled_node` — 無効時のサイレント出力
- `test_set_node_position` — ノード位置設定
- `test_set_node_enabled_bypassed` — 有効/バイパス切替
- `test_already_connected` — 重複接続の拒否
- `test_connect_nonexistent_node` — 存在しないノードへの接続拒否
- `test_session_default_has_no_graph` — デフォルトセッションにグラフなし
- `test_session_roundtrip` — セッション保存→読み込みの一貫性
- `test_legacy_migration` — 旧チェーン→グラフ自動マイグレーション
- `test_empty_chain_no_migration` — 空チェーンではマイグレーションなし
- `test_legacy_migration_chain_order` — マイグレーション接続順序の検証

### Phase 2 残り 〜 Phase 4: 未実装

| Phase | 内容 | 状態 |
|-------|------|------|
| 2-3 | ダブルバッファ戦略（arc_swap、ロックフリー処理） | ✅ 完了 |
| 2-4 | Looper/Metronome のノード化 | ✅ 完了 |
| 2-5 | 後方互換セッション読み込み | ✅ 完了 |
| 3-1 | ノードエディタUI（基礎） | ✅ 完了 |
| 3-2 | ノードエディタUI（パラメータ編集・接続削除・複製・ズームフィット） | ✅ 完了 |
| 3-3 | ノードエディタUI（VSTプラグイン統合） | ✅ 完了 |
| Phase 4 | 高度なルーティング（セッション保存/復元・マイグレーション） | ✅ 完了 |

### Phase 3-1: ノードエディタUI（基礎） ✅ 完了

**src/ui/node_editor.rs** — 新規ファイル（キャンバスベースのノードグラフエディタ）:
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

**src/app.rs 変更点**:
- `ViewMode` enum 追加（Rack / NodeEditor）
- `ToneDockApp` に `view_mode`, `node_editor` フィールド追加
- ツールバーに「Node Editor」/「Rack View」切替ボタン追加
- CentralPanel を ViewMode に応じて切替:
  - `show_rack_view()` — 従来のラックビュー（リファクタリング）
  - `show_node_editor()` — ノードエディタ＋サイドメーター
- `process_editor_commands()` — EdCmd をエンジンメソッド呼び出しに変換

**src/audio/graph.rs 変更点**:
- `GraphNode` に `looper_buffer: Mutex<Option<LooperBuffer>>` フィールドを追加
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

**src/app.rs 変更点**:
- UI制御をスタンドアローン `Metronome`/`Looper` からグラフノード経由に完全移行
- メトロノーム: `graph_set_state()` で BPM/Volume を `MetronomeNodeState` に設定
- ルーパー: `graph_set_state()` で enabled/recording/playing/overdubbing/cleared を制御
- `graph_set_enabled()` + `graph_commit_topology()` でノードの有効/無効を管理
- ループ長表示: `graph.load().looper_loop_length()` から取得

### Phase 3-2: ノードエディタUI（パラメータ編集・接続削除・複製・ズームフィット） ✅ 完了

**src/ui/node_editor.rs 変更点**:

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

**src/app.rs 変更点**:
- `process_editor_commands()` に3つの新コマンドハンドラ追加:
  - `EdCmd::Disconnect` — `graph_disconnect()` 呼び出し
  - `EdCmd::SetState` — `graph_set_state()` 呼び出し
  - `EdCmd::DuplicateNode` — グラフからノード情報取得 → 新ノード追加 → 状態コピー → 選択

**paint_bezier() シグネチャ変更**:
- `width: f32` パラメータ追加 — ホバー時の太線表示に対応

### Phase 3-3: ノードエディタUI（VSTプラグイン統合） ✅ 完了

**src/ui/node_editor.rs 変更点**:

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

**src/app.rs 変更点**:

- **`show_node_editor()`**:
  - `node_editor.show()` に `available_plugins` を渡すよう変更
  - サイドパネルに `draw_vst_parameter_panel()` を追加（選択ノードがVSTの場合）
- **`process_editor_commands()`** に2つの新コマンドハンドラ追加:
  - `EdCmd::AddVstNode` — NodeType::VstPlugin ノード追加 → `load_vst_plugin_to_node()` でプラグインロード
  - `EdCmd::SetVstParameter` — `set_vst_node_parameter()` 呼び出し
- **`draw_vst_parameter_panel()`** 新規メソッド:
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

**src/app.rs**:
- `build_session()` — 現在の AudioGraph から SerializedGraph を生成して保存
- `load_session()` — 読み込み後に `load_serialized_graph()` でグラフを復元

**src/audio/engine.rs**:
- `load_serialized_graph()` — SerializedGraph から AudioGraph を復元（ID マッピング、シングルトン制約、トポロジコミット、ArcSwap store）

## 次にやること

### Phase 5: 高度なルーティング（Send/Return、Wet/Dry）

設計書 `docs/node_based_routing_design.md` の Phase 4 に該当する機能。現在未実装。

- **Send/Return バス** — エフェクトのセンド/リターンルーティング
- **Wet/Dry ノード** — エフェクトのミックス比制御
- **パラレルチェーンのテンプレート** — "Wide Stereo Amp" 等のワンタップテンプレート
- **ゼロコピースプリッター** — 出力バッファの参照共有でメモリコピー削減

### その他の改善候補

- **Rack View ↔ Node Editor 双方向同期** — 現在は独立して操作可能。同じ AudioGraph を操作する異なるビューとして統合
- **Undo/Redo** — グラフ操作の履歴管理
- **ノードエディタのマルチ選択** — 複数ノードの同時移動・削除
- **VST パラメータオートメーション** — パラメータの時間変化記録

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
- ゼロコピースプリッターの実装は Phase 4 で検討
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
