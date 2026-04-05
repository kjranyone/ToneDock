# ToneDock — Handover資料

最終更新: 2026-04-06 02:35 JST

## プロジェクト概要

**ToneDock** は Rust で書くギター練習用 VST3 ホストアプリです。GPL-3.0 ライセンス。

- リポジトリ: `C:\lib\github\kjranyone\ToneDock`
- `cargo check` **通ります** / `cargo test` **13テスト全パス**
- 設計書: `docs/node_based_routing_design.md`

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
│   ├── engine.rs         — cpalオーディオエンジン（Chain使用中）
│   ├── node.rs           — **[新規]** ノード型定義（NodeId, NodeType, Port, etc.）
│   └── graph.rs          — **[新規]** AudioGraph（DAG処理、トポロジカルソート、バッファ管理）
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

### Phase 2〜4: 未実装

| Phase | 内容 | 状態 |
|-------|------|------|
| Phase 2 | エンジン統合（Chain→AudioGraph置換、Command Queue、ダブルバッファ） | 未着手 |
| Phase 3 | ノードエディタUI | 未着手 |
| Phase 4 | 高度なルーティング（Send/Return、Wet/Dry） | 未着手 |

## 次にやること（Phase 2: エンジン統合）

### 2-1: AudioEngine の Chain → AudioGraph 置換
- `engine.rs` の `Arc<Mutex<Chain>>` を `Arc<Mutex<AudioGraph>>` に変更
- オーディオコールバック内の処理フローを `graph.process()` に変更
- 従来の `Chain` は残す（後方互換）

### 2-2: Command Queue パターン
- UI→Audio スレッド間のコマンド送信に `crossbeam::channel` を使用
- `GraphCommand` enum（AddNode, RemoveNode, Connect, Disconnect, SetParameter等）
- オーディオスレッドではロック・アロケーション禁止

### 2-3: ダブルバッファ戦略
- UIスレッドでグラフ構造をクローン→変更→トポロジ計算
- `Arc<AtomicPtr>` または `arc_swap` でアトミックにスワップ
- オーディオスレッドは不変参照で処理

### 2-4: Looper/Metronome のノード化
- 現在のスタンドアローン `Looper`/`Metronome` を AudioGraph 内のノードに統合
- 既存のUI制御（app.rs）をグラフノード経由に更新

### 2-5: 後方互換セッション読み込み
- 旧 `Vec<ChainSlot>` 形式のセッションを AudioGraph 形式に自動変換
- `migrate_legacy_session()` を実装

## 技術的メモ

### Rust 2024 Edition 注意点
- `ref` パターンは暗黙借用の対象になるため `if let Some(Some(ref x))` → `if let Some(Some(x))` にする必要がある
- `split_at_mut()` を使って `&mut Vec` の要素間で同時借用可能にする

### バッファ戦略
- `gather_inputs()` で `clone()` を多用している（将来的に最適化可能）
- ゼロコピースプリッターの実装は Phase 4 で検討
- `max_frames` はデフォルト 256、バッファサイズ変更時に全ノードのリサイズが必要

### Mono-In / Stereo-Out 原則
- AudioInput: 常に 1ch (Mono)
- AudioOutput: 常に 2ch (Stereo)
- 自動チャンネル変換は `connect()` 時に許可し、`gather_inputs()` で実行
