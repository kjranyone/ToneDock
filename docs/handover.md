# ToneDock — Handover資料

最終更新: 2026-04-06 08:05 JST

## プロジェクト概要

**ToneDock** は Rust で書くギター練習用 VST3 ホストアプリです。GPL-3.0 ライセンス。

- リポジトリ: `C:\lib\github\kjranyone\ToneDock`
- `cargo check` **通ります** / `cargo test` **13テスト全パス**
- 設計書: `docs/node_based_routing_design.md`
- 新依存: `arc-swap = "1"` （Phase 2-3で追加）

## 現在のアーキテクチャ

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

### テスト結果（13テスト全パス）
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

### Phase 2 残り 〜 Phase 4: 未実装

| Phase | 内容 | 状態 |
|-------|------|------|
| 2-3 | ダブルバッファ戦略（arc_swap、ロックフリー処理） | ✅ 完了 |
| 2-4 | Looper/Metronome のノード化 | 未着手 |
| 2-5 | 後方互換セッション読み込み | 未着手 |
| Phase 3 | ノードエディタUI | 未着手 |
| Phase 4 | 高度なルーティング（Send/Return、Wet/Dry） | 未着手 |

## 次にやること（Phase 2-4: Looper/Metronome のノード化）

- 現在のスタンドアローン `Looper`/`Metronome` を AudioGraph 内のノードに統合
- 既存のUI制御（app.rs）をグラフノード経由に更新
- メトロノームノード: BPM/Volume を `NodeInternalState` 経由で制御、`process_metronome_node()` は既に実装済み
- ルーパーノード: Rec/Play/Overdub/Clear を `NodeInternalState` 経由で制御、現在はパススルー（本格実装が必要）

### 2-5: 後方互換セッション読み込み
- 旧 `Vec<ChainSlot>` 形式のセッションを AudioGraph 形式に自動変換
- `migrate_legacy_session()` を実装

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
