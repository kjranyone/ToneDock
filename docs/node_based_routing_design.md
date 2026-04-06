# ToneDock ノードベースオーディオルーティング設計書

## 1. 概要

### 1.1 目的

ToneDockの現行アーキテクチャは `Vec<PluginSlot>` による直列チェーンのみをサポートしており、
信号の分岐・合流・パラレル処理が不可能である。本ドキュメントでは、
**DAG（有向非巡回グラフ）ベースのノードルーティングシステム**の設計を定義する。

### 1.2 設計目標

- **自由な音作り**: エフェクトの分岐・合流・パラレルチェーンを可能にする
- **Mono-In / Stereo-Out**: ギター入力はモノラル(1ch)→分岐→各パスでPAN指定→ステレオ(2ch)出力
- **リアルタイム安全性**: オーディオスレッドでのアロケーション・ロックなし
- **段階的導入**: 既存の直列チェーンとの後方互換性を維持
- **直感的UI**: ノードエディタによる視覚的なルーティング
- **低オーバーヘッド**: グラフ管理が音質・レイテンシに影響しないこと

### 1.4 基本前提: Mono-In / Stereo-Out

ギター練習アプリケーションの信号の基本形：

```
 Guitar (1ch)          Processing (各パス任意)           Output (2ch)
     ┌──────┐                                            ┌──────────┐
 ──► │ Mono │──┬──────► Amp A ──► Pan L ◄──┬──────────►│ Stereo   │──► L
     │Input │  │                           │            │ Output   │──► R
     └──────┘  └──────► Amp B ──► Pan R ◄──┘            └──────────┘
```

- **AudioInput**: 常にモノラル(1ch)
- **AudioOutput**: 常にステレオ(2ch)
- **VST プラグイン**: 入力チャンネル数に応じて自動的に Mono↔Stereo 変換を行う
- **Pan ノード**: モノラル(1ch)を入力に、ステレオ(2ch)を出力する。pan値でL/Rのバランスを制御
- **分岐(Split)**: 同じモノラル信号を複数パスに分配し、各パスで独立したエフェクト処理＋PANが可能

### 1.3 用語定義

| 用語 | 説明 |
|------|------|
| **Node** | 1つの処理単位（VSTプラグイン、入力/出力、ミキサー等） |
| **Port** | ノードの入出力端子。チャンネル数（モノラル/ステレオ）を持つ |
| **Connection** | 出力ポートから入力ポートへの接続（エッジ） |
| **AudioGraph** | ノードとコネクションの集合。DAG制約を満たす |
| **Topology** | グラフの処理順序（トポロジカルソート結果） |
| **Bus** | 1つのポートが扱うオーディオバッファ（planar f32、1〜Nチャンネル） |

---

## 2. 現行アーキテクチャの分析

### 2.1 現行の信号フロー

```
Audio Input → Input Gain → [Plugin 0] → [Plugin 1] → ... → [Plugin N]
                                                       ↓
                                                    Looper
                                                       ↓
                                                    Metronome
                                                       ↓
                                                   Master Vol
                                                       ↓
                                                   Audio Output
```

### 2.2 現行の制約

| 制約 | 影響 |
|------|------|
| 直列 `Vec<PluginSlot>` のみ | 分岐・合流が不可能 |
| in-place処理のみ | 1入力1出力のプラグインしか扱えない |
| Looper/Metronomeが固定位置 | Pre/Postルーパーの切替が制限的 |
| バッファ共有なし | パラレルパスで同じ入力を分配できない |

### 2.3 変更対象ファイル

| ファイル | 変更内容 |
|----------|----------|
| `src/audio/chain.rs` | `Chain` → `AudioGraph` への置換 |
| `src/audio/engine.rs` | エンジンの処理ループをグラフベースに変更 |
| `src/audio/mod.rs` | 新モジュール `graph.rs`, `node.rs` を追加 |
| `src/session.rs` | グラフ構造のシリアライズ対応 |
| `src/app.rs` | UI統合（ノードエディタモード） |
| `src/ui/rack_view.rs` | ラックビューに代わるノードエディタビュー |
| `src/ui/mod.rs` | `node_editor.rs` を追加 |

---

## 3. データ構造設計

### 3.1 コア型定義

```rust
// src/audio/node.rs

/// ノードの一意な識別子
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// ポートの一意な識別子（ノード内のインデックス）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortId(pub u32);

/// ポートの種類
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

/// ポートが扱うオーディオチャンネル構成
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelConfig {
    Mono,       // 1ch — ギター入力、モノラルエフェクト
    Stereo,     // 2ch (L, R) — 出力、ステレオエフェクト
    Custom(u16), // 任意ch数（将来拡張用）
}

/// ノードの種類
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    /// オーディオ入力デバイス（常にモノラル: 1ch）
    AudioInput,
    /// オーディオ出力デバイス（常にステレオ: 2ch）
    AudioOutput,
    /// VST3プラグイン
    VstPlugin {
        plugin_path: String,
        plugin_name: String,
    },
    /// ミキサーノード（複数入力を合流）
    Mixer { inputs: u16 },
    /// スプリッターノード（1入力を複数出力へ分配）
    Splitter { outputs: u16 },
    /// パンニングノード（Mono→Stereo変換 + L/Rバランス制御）
    Pan,
    /// チャンネル変換（Mono→Stereo: 1ch→2ch 複製、Stereo→Mono: 2ch→1ch 平均）
    ChannelConverter { target: ChannelConfig },
    /// メトロノーム
    Metronome,
    /// ルーパー
    Looper,
    /// ゲイン/ボリューム
    Gain,
}
```

### 3.2 ポート定義

```rust
/// ノードのポート（入力または出力端子）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub id: PortId,
    pub name: String,
    pub direction: PortDirection,
    pub channels: ChannelConfig,
}
```

### 3.3 コネクション（エッジ）

```rust
/// ノード間の接続
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub source_node: NodeId,
    pub source_port: PortId,
    pub target_node: NodeId,
    pub target_port: PortId,
}
```

### 3.4 グラフノード

```rust
/// グラフ内の1ノード
pub struct GraphNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub input_ports: Vec<Port>,
    pub output_ports: Vec<Port>,
    pub enabled: bool,
    pub bypassed: bool,

    // 処理用バッファ（トポロジカルソート後に割り当て）
    input_buffers: Vec<Option<Vec<Vec<f32>>>>,  // None = 未接続
    output_buffers: Vec<Vec<Vec<f32>>>,          // 各ポートの出力

    // VSTプラグイン固有
    pub plugin_instance: Option<LoadedPlugin>,

    // 内部状態（メトロノーム、ルーパー等）
    // → 各NodeTypeに応じた内部状態はtrait/enumで管理
}
```

### 3.5 AudioGraph

```rust
/// オーディオ信号グラフ（DAG）
pub struct AudioGraph {
    nodes: HashMap<NodeId, GraphNode>,
    connections: Vec<Connection>,

    // 処理順序（トポロジカルソート結果のキャッシュ）
    process_order: Vec<NodeId>,

    // 特殊ノードのID
    input_node_id: Option<NodeId>,
    output_node_id: Option<NodeId>,

    // バッファ管理
    max_frames: usize,
    sample_rate: f64,

    // ダーティフラグ（構造変更時にトポロジ再計算が必要）
    topology_dirty: bool,
}
```

---

## 4. グラフ処理アルゴリズム

### 4.1 トポロジカルソート

構造変更時にトポロジカルソートを実行し、処理順序を決定する。

```
Kahn's Algorithm:
1. 全ノードの入力次数を計算
2. 入力次数0のノードをキューに追加
3. キューからノードを取り出し、結果に追加
4. そのノードの出力エッジを削除し、接続先の入力次数をデクリメント
5. 入力次数が0になったノードをキューに追加
6. キューが空になるまで繰り返す
7. 結果に全ノードが含まれていなければサイクル検知（エラー）
```

### 4.2 バッファ管理

#### 4.2.1 バッファ割り当て戦略

トポロジカルソート完了後、各ノードのポートにバッファを割り当てる。

- **出力ポート**: 各ポートに独立したバッファを割り当て
- **入力ポート**: 接続元の出力バッファへの参照（ゼロコピー）
- **未接続入力**: サイレントバッファ（ゼロ埋め）を割り当て

#### 4.2.2 ミキシング（多対1接続）

複数の出力ポートが1つの入力ポートに接続されている場合、
**最後の接続元の処理後にミキシング**を行う。

```rust
fn mix_inputs(target_buffer: &mut Vec<Vec<f32>>, sources: &[&Vec<Vec<f32>>) {
    // 最初のソースをコピー
    if let Some(first) = sources.first() {
        for (ch, buf) in target_buffer.iter_mut().enumerate() {
            buf.copy_from_slice(&first[ch][..]);
        }
    }
    // 残りのソースを加算
    for source in sources.iter().skip(1) {
        for (ch, buf) in target_buffer.iter_mut().enumerate() {
            for (i, s) in source[ch].iter().enumerate() {
                buf[i] += s;
            }
        }
    }
}
```

### 4.3 処理フロー

```
AudioGraph::process(input_buffer, num_frames):

1. トポロジがdirty → 再計算（UIスレッドで実行済みが前提）
2. input_buffer を AudioInput ノードの出力ポートにコピー
3. process_order 順に各ノードを処理:
   a. 入力ポートに接続されたバッファを集約（ミキシング）
   b. ノードの process() を実行
   c. 出力ポートのバッファが後続ノードから参照可能に
4. AudioOutput ノードの出力をメイン出力バッファにコピー
5. （Looper, Metronome はノードとしてグラフ内で処理される）
```

### 4.4 処理シーケンス図（例: パラレルアンプ + PAN）

```
  ┌──────────────┐
  │ AudioInput   │
  │  ● out (mono)│
  └──────┬───────┘
         │ (1ch)
    ┌────┴────┐                  ← Splitter
    │         │
    │ (1ch)   │ (1ch)
 ┌──▼───────┐ ┌▼─────────┐
 │  Amp A   │ │  Amp B   │       ← パラレルチェーン
 │  Delay   │ │  Reverb  │
 └──┬───────┘ ┌┬─────────┘
    │ (1ch)   │ (1ch)
 ┌──▼───────┐ ┌▼─────────┐
 │  Pan L   │ │  Pan R   │       ← Mono→Stereo変換 + PAN
 │  pan=-0.8│ │  pan=+0.8│
 └──┬───────┘ ┌┬─────────┘
    │ (2ch)   │ (2ch)
    └────┬────┘                  ← Mixer (2ch + 2ch → 2ch)
         │ (2ch)
  ┌──────▼───────┐
  │  IR Loader   │
  └──────┬───────┘
         │ (2ch)
  ┌──────▼───────┐
  │ AudioOutput  │
  │  ● in (stereo)│
  └──────────────┘
```

**ポイント**: Splitter でモノラル(1ch)を2系統に分岐 → 各系統で独立したエフェクト処理 →
Pan ノードで Mono→Stereo 変換とL/R定位 → Mixer で合流 → Stereo 出力。

---

## 5. ノードタイプ詳細

### 5.1 AudioInput / AudioOutput

| 項目 | AudioInput | AudioOutput |
|------|-----------|-------------|
| 入力ポート | なし | 1個（ステレオ） |
| 出力ポート | 1個（**モノラル**） | なし |
| 処理 | 外部入力バッファをコピー | 出力を外部出力バッファにコピー |
| インスタンス | グラフ内に1つのみ | グラフ内に1つのみ |
| チャンネル | **常に 1ch（モノラル）** | **常に 2ch（ステレオ）** |

AudioInput はギターのモノラル信号を1chバッファとして出力する。
AudioOutput はステレオ2chを前提とし、モノラル入力が接続された場合は自動的に
L=R複製でステレオ化する。

### 5.2 VstPlugin

| 項目 | 説明 |
|------|------|
| 入力ポート | プラグインの入力バス数に依存 |
| 出力ポート | プラグインの出力バス数に依存 |
| 処理 | `IAudioProcessor::process()` を呼び出し |
| パラメータ | `IEditController` 経由でアクセス |
| bypass時 | 入力を出力にコピー（パススルー） |
| disable時 | ゼロ出力（ミュート） |
| チャンネル変換 | 接続時に自動で Mono↔Stereo 変換（後述） |

### 5.3 Pan（パンニング）

**Mono(1ch) → Stereo(2ch) 変換 + L/R バランス制御**

ギターのモノラル信号をステレオ空間の任意の位置に配置する。

| 項目 | 説明 |
|------|------|
| 入力ポート | 1個（**モノラル**: 1ch） |
| 出力ポート | 1個（**ステレオ**: 2ch） |
| パラメータ | `pan: f32`（-1.0 = Full Left, 0.0 = Center, +1.0 = Full Right） |
| 処理 | モノラル入力を pan 値に基づいて L/R に分配 |

```
pan = -1.0 (Full Left):   L = input,  R = 0
pan =  0.0 (Center):      L = input × 0.707,  R = input × 0.707  (等パワーパン)
pan = +1.0 (Full Right):  L = 0,      R = input
pan = -0.5:               L = input × cos(π/6), R = input × sin(π/6)
```

**等パワーパン（constant-power panning）の数式:**

```rust
let angle = (pan_value + 1.0) * std::f32::consts::FRAC_PI_4; // 0..π/2
let gain_l = angle.cos();
let gain_r = angle.sin();

output[0][i] = input[0][i] * gain_l;  // L
output[1][i] = input[0][i] * gain_r;  // R
```

### 5.4 ChannelConverter（チャンネル数変換）

| 項目 | 説明 |
|------|------|
| 入力ポート | 1個（Mono or Stereo） |
| 出力ポート | 1個（変換後のチャンネル数） |
| Mono→Stereo | L = input, R = input（同一信号を複製） |
| Stereo→Mono | output = (L + R) / 2（L/R平均） |

### 5.5 Splitter（スプリッター）

| 項目 | 説明 |
|------|------|
| 入力ポート | 1個（入力と同じチャンネル数） |
| 出力ポート | N個（入力と同じチャンネル数を複製） |
| 処理 | 入力を全出力にコピー（ゼロコピー参照推奨） |
| 典型用途 | モノラルギター入力を2系統に分岐し、別々のアンプシミュ＋PANでL/Rに配置 |

### 5.6 Mixer（ミキサー）

| 項目 | 説明 |
|------|------|
| 入力ポート | N個（全ポート同一チャンネル数） |
| 出力ポート | 1個（入力と同じチャンネル数） |
| 処理 | 全入力を加算して出力 |
| 典型用途 | パラレルチェーンの合流（Pan L + Pan R → Stereo Mixer） |

### 5.7 接続時のチャンネル変換ルール

異なるチャンネル数のポート間を接続する場合、**接続点に暗黙の変換**を自動挿入する。

| 接続元 → 接続先 | 自動処理 |
|-----------------|----------|
| Mono(1ch) → Mono(1ch) | そのまま |
| Stereo(2ch) → Stereo(2ch) | そのまま |
| Mono(1ch) → Stereo(2ch) | **自動複製**: L=input, R=input |
| Stereo(2ch) → Mono(1ch) | **自動ダウンミックス**: out=(L+R)/2 |

この暗黙変換はコネクションのミキシング時に行われ、ユーザーが明示的に
`ChannelConverter` ノードを追加する必要はない（ただし明示的な挿入も可能）。

### 5.5 内蔵ノード（Metronome, Looper, Gain）

これらは `NodeType` のバリアントとして定義し、
各ノードの内部状態は `GraphNode` の拡張フィールドで管理する。

```rust
pub enum NodeInternalState {
    None,
    Metronome(MetronomeState),
    Looper(LooperState),
    Gain { value: f32 },
    Pan { value: f32 },  // -1.0 (Full Left) .. +1.0 (Full Right)
}
```

---

## 6. グラフ操作API

### 6.1 AudioGraph の公開メソッド

```rust
impl AudioGraph {
    // === 構造操作（UIスレッドから呼び出し） ===

    /// 新しいノードを追加
    fn add_node(&mut self, node_type: NodeType) -> NodeId;

    /// ノードを削除（接続も自動削除）
    fn remove_node(&mut self, id: NodeId);

    /// ノード間にコネクションを追加
    /// 戻り値: Ok(()) or Err(GraphError)
    /// エラー条件: サイクル検出、チャンネル数不一致、ポート重複接続
    fn connect(&mut self, conn: Connection) -> Result<(), GraphError>;

    /// コネクションを削除
    fn disconnect(&mut self, source: (NodeId, PortId), target: (NodeId, PortId));

    /// ノードの有効/無効切替
    fn set_node_enabled(&mut self, id: NodeId, enabled: bool);

    /// ノードのバイパス切替
    fn set_node_bypassed(&mut self, id: NodeId, bypassed: bool);

    /// ノードの位置移動（UI座標、処理には影響しない）
    fn set_node_position(&mut self, id: NodeId, x: f32, y: f32);

    // === 処理（オーディオスレッドから呼び出し） ===

    /// トポロジを確定（dirty時にUIスレッドで呼び出す）
    fn commit_topology(&mut self) -> Result<(), GraphError>;

    /// オーディオフレームを処理
    fn process(&mut self, input: &[Vec<f32>], num_frames: usize) -> Vec<Vec<f32>>;
}
```

### 6.2 エラー型

```rust
#[derive(Debug)]
pub enum GraphError {
    /// 接続によりサイクルが発生する
    CycleDetected,
    /// ポートのチャンネル数が一致しない
    ChannelMismatch { source: ChannelConfig, target: ChannelConfig },
    /// 指定されたノード/ポートが存在しない
    NotFound,
    /// 同じターゲットポートに既に接続されている（許容する場合は無視）
    AlreadyConnected,
    /// AudioInput/AudioOutput が既に存在する
    SingletonViolation,
}
```

---

## 7. スレッド安全性設計

### 7.1 スレッドモデル

```
┌─────────────────────┐     ┌─────────────────────────────┐
│    UI Thread         │     │    Audio Thread              │
│                      │     │                              │
│  - グラフ構造の編集   │     │  - process() の実行          │
│  - トポロジの再計算   │     │  - バッファの読み書き         │
│  - パラメータ変更     │     │  - ノードの process() 呼び出し│
│                      │     │                              │
│  変更は command キュー│────►│  フレーム間で command を消費  │
│  に積まれる           │     │                              │
└─────────────────────┘     └─────────────────────────────┘
```

### 7.2 Command Queue パターン

リアルタイムスレッドでのロック・アロケーションを避けるため、
`crossbeam::channel` 経由でコマンドを送信する。

```rust
pub enum GraphCommand {
    AddNode { id: NodeId, node_type: NodeType },
    RemoveNode { id: NodeId },
    Connect { conn: Connection },
    Disconnect { source_node: NodeId, source_port: PortId, target_node: NodeId, target_port: PortId },
    SetEnabled { id: NodeId, enabled: bool },
    SetBypassed { id: NodeId, bypassed: bool },
    SetParameter { node_id: NodeId, param_id: u32, value: f32 },
}
```

### 7.3 ダブルバッファ戦略

グラフ構造の変更は、UIスレッドで新しいトポロジを構築し、
フレーム境界でアトミックにスワップする。

```
UI Thread:
  1. 現在の graph をクローン
  2. クローンに対して変更を適用
  3. トポロジカルソートを実行
  4. バッファを割り当て
  5. Arc::new() でラップ
  6. atomic swap で audio thread の参照を更新

Audio Thread:
  1. 各フレーム開始時に atomic load でグラフ参照を取得
  2. その参照を使って処理
  3. フレーム中はグラフ参照が不変であることを保証
```

---

## 8. UI設計

### 8.1 ノードエディタの構成

```
┌──────────────────────────────────────────────────────────────────┐
│  ToneDock - Node Editor                                [Rack ▼] │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌───────────┐    ┌───────────┐    ┌───────────┐                │
│  │ Audio In  │───►│  Amp Sim  │───►│ IR Loader │──┐             │
│  │ ● out (M) │    │ ●in(M)out │    │ ●in(M)out │  │             │
│  └───────────┘    └───────────┘    └───────────┘  │             │
│       │                                           │             │
│       │         ┌───────────┐    ┌───────────┐    │             │
│       └────────►│  Delay    │───►│  Pan R    │    │             │
│                 │ ●in(M)out │    │ ●in(M)outS│────┤             │
│                 └───────────┘    │ pan=+0.8  │    │             │
│                                  └───────────┘    │             │
│  ┌───────────┐    ┌───────────┐    ┌───────────┐  │             │
│  │ Audio In  │───►│  Reverb   │───►│  Pan L    │  │             │
│  │ ● out (M) │    │ ●in(M)out │    │ ●in(M)outS│──┐│             │
│  └───────────┘    └───────────┘    │ pan=-0.8  │  ││             │
│                                    └───────────┘  ││             │
│                                           ┌──────▼▼▼──┐         │
│                                           │   Mixer   │         │
│                                           │ ●● in  out│         │
│                                           └─────┬─────┘         │
│                                                 │                │
│                                           ┌─────▼─────┐         │
│                                           │ Audio Out │         │
│                                           │ ● in (S)  │         │
│                                           └───────────┘         │
├──────────────────────────────────────────────────────────────────┤
│  [+ Add Node]  │ Transport Controls │ Properties Panel           │
└──────────────────────────────────────────────────────────────────┘

  M = Mono (1ch)   S = Stereo (2ch)
```

### 8.2 UI操作

| 操作 | 説明 |
|------|------|
| ドラッグ（背景） | キャンバスのパン |
| スクロール | ズームイン/アウト |
| ドラッグ（ポート） | ポートから線を引き出し、接続先ポートでドロップ |
| ダブルクリック（ノード） | パラメータパネルを開く |
| 右クリック | コンテキストメニュー（ノード追加、削除等） |
| Ctrl+D | 選択ノードを複製 |
| Delete | 選択ノード/コネクションを削除 |

### 8.3 ビルドインUIノード

各ノードの見た目：

```
┌─────────────────────┐
│ 🎸 Amp Sim      [⚡]│  ← ノード名、バイパスボタン
│─────────────────────│
│ ● in (M)            │  ← 入力ポート（左側）、M=Mono
│               out (M)● │  ← 出力ポート（右側）、M=Mono
│─────────────────────│
│ Gain: ═══●═══  0.7dB│  ← 主要パラメータ（省スペース）
│ Tone: ═══●═══  0.3  │
└─────────────────────┘

┌─────────────────────┐
│ ◉ Pan           [⚡]│  ← Pan ノード（Mono→Stereo変換）
│─────────────────────│
│ ● in (M)            │
│               out (S)● │  ← M入力→S出力
│─────────────────────│
│ Pan: ═══●═══  L 0.8 │  ← -1.0(L) ... 0.0(C) ... +1.0(R)
└─────────────────────┘
```

ポートにはチャンネル数を示すラベル `(M)` = Mono, `(S)` = Stereo が表示される。
接続時にチャンネル数が異なる場合は、自動変換マーク `⟿` が接続線上に表示される。

### 8.4 ビューモード切替

ユーザーは以下のビューを切り替え可能：

1. **Rack View**（現行）: 直感的なリスト形式。初心者向け。
2. **Node Editor View**（新規）: 自由なルーティング。中級〜上級者向け。

両ビューは同じ `AudioGraph` を操作する異なる表現であり、
Rack Viewは内部でノードグラフを直列に構築・表示する。

---

## 9. シリアライズ設計

### 9.1 セッションフォーマット拡張

```rust
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub sample_rate: f64,
    pub buffer_size: u32,
    pub graph: SerializedGraph,
}

#[derive(Serialize, Deserialize)]
pub struct SerializedGraph {
    pub nodes: Vec<SerializedNode>,
    pub connections: Vec<Connection>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializedNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub enabled: bool,
    pub bypassed: bool,
    pub position: (f32, f32),
    pub parameters: Vec<(u32, f32)>,
}
```

### 9.2 後方互換性

旧セッション（`Vec<ChainSlot>`）の読み込み：

```rust
fn migrate_legacy_session(legacy: LegacySession) -> Session {
    let mut graph = SerializedGraph {
        nodes: vec![],
        connections: vec![],
    };

    // AudioInput ノードを追加
    let input_id = NodeId(0);
    graph.nodes.push(SerializedNode::audio_input(input_id));

    // 各 ChainSlot を VstPlugin ノードに変換し直列接続
    let mut prev_id = input_id;
    for (i, slot) in legacy.chain.iter().enumerate() {
        let node_id = NodeId((i + 1) as u64);
        graph.nodes.push(SerializedNode::from_chain_slot(node_id, slot));
        graph.connections.push(Connection {
            source_node: prev_id,
            source_port: PortId(0),
            target_node: node_id,
            target_port: PortId(0),
        });
        prev_id = node_id;
    }

    // AudioOutput ノードを追加
    let output_id = NodeId((legacy.chain.len() + 1) as u64);
    graph.nodes.push(SerializedNode::audio_output(output_id));
    graph.connections.push(Connection {
        source_node: prev_id,
        source_port: PortId(0),
        target_node: output_id,
        target_port: PortId(0),
    });

    Session { graph, ..legacy.into() }
}
```

---

## 10. 実装フェーズ

### Phase 1: コアデータ構造（1〜2週間）

- [x] `src/audio/node.rs` — `NodeId`, `PortId`, `Port`, `NodeType`, `NodeInternalState`
- [x] `src/audio/graph.rs` — `AudioGraph`, `Connection`, `GraphNode`
- [x] トポロジカルソートの実装
- [x] バッファ管理（割り当て・ミキシング）
- [x] 単体テスト（サイクル検出、ソート順序、バッファ割り当て）

### Phase 2: エンジン統合（1〜2週間）

- [x] `AudioEngine` の `Chain` を `AudioGraph` に置換
- [x] Command Queue パターンの実装
- [x] ダブルバッファ戦略の実装
- [x] 現行のLooper/Metronomeをノード化
- [x] 後方互換セッション読み込み

### Phase 3: ノードエディタUI（2〜3週間）

- [x] `src/ui/node_editor.rs` — キャンバスベースのノードエディタ
- [x] ポートのドラッグ＆ドロップ接続
- [x] コンテキストメニュー（ノード追加/削除）
- [x] ノードのパラメータ表示・編集
- [x] ズーム/パン操作
- [x] Rack View ↔ Node Editor 切替

### Phase 4: 高度なルーティング（1〜2週間）

- [x] Splitter / Mixer ノードの実装
- [x] Pan ノードの実装（等パワーパンニング）
- [x] 自動チャンネル変換（Mono↔Stereo 暗黙変換）
- [ ] Send/Return バス（将来的）
- [ ] Wet/Dry ノード（ミックス比制御）
- [ ] パラレルチェーンのテンプレート

---

## 11. パラレルPANテンプレート例

ユーザーが「ワンタップ」で使える典型的なパラレルPAN構成テンプレート。

### テンプレート: "Wide Stereo Amp"

```
AudioInput (M)
    │
    ├──► Amp Sim A ──► Pan (L=-0.8) ──┐
    │                                  ├──► Mixer ──► IR Loader ──► AudioOutput (S)
    └──► Amp Sim B ──► Pan (R=+0.8) ──┘
```

### テンプレート: "Mono Amp + Stereo Reverb"

```
AudioInput (M)
    │
    └──► Amp Sim ──► IR Loader ──► ChannelConverter (M→S)
                                          │
                                          └──► Stereo Reverb ──► AudioOutput (S)
```

### テンプレート: "Dry/Wet Blend"

```
AudioInput (M)
    │
    ├──► [何もしない] ──────────────────────┐
    │                                        ├──► Mixer ──► AudioOutput (S)
    └──► Amp Sim ──► Pan (Center) ──► Reverb┘
```

---

## 11. パフォーマンス考慮事項

### 11.1 メモリ使用量

| 項目 | 推定値 |
|------|--------|
| バッファ per ポート | 2ch × 256samples × 4bytes = 2KB |
| 10ノード × 2ポート平均 | 40KB |
| トポロジ計算 | O(V + E)、実質無視可能 |

### 11.2 処理オーバーヘッド

- トポロジカルソートは構造変更時のみ実行（フレーム毎ではない）
- バッファのミキシングは SIMD 最適化の余地あり
- ゼロコピースプリッター（出力バッファの参照共有）でメモリコピーを削減

### 11.3 レイテンシ

- グラフ処理自体は追加レイテンシを生まない（同フレーム内で完結）
- プラグイン自体の internal latency は変更なし
- バッファサイズは従来通りユーザー設定に依存

---

## 12. リスクと対策

| リスク | 対策 |
|--------|------|
| サイクル検出の漏れ | `connect()` 時に必ずチェック。トポロジ確定時も二重チェック |
| リアルタイムスレッドの安全性 | Command Queue パターン。オーディオスレッドではロック・アロケーション禁止 |
| VST プラグインの多入出力対応 | Phase 1では 1in/1out のみサポート。多バスプラグインは将来対応 |
| UIの複雑化 | 初心者向け Rack View を維持。Node Editor はオプション |
| 既存セッションの互換性破壊 | マイグレーション関数で自動変換 |

---

## 13. まとめ

この設計により、ToneDockは以下を実現する：

1. **自由な音作り**: ノードベースのルーティングで分岐・合流・パラレル処理が可能
2. **段階的導入**: Rack View（直列）と Node Editor（自由）のデュアルモード
3. **リアルタイム安全**: Command Queue + ダブルバッファでロックフリー処理
4. **後方互換**: 既存セッションを自動的にグラフ形式にマイグレーション
5. **拡張性**: 新しい NodeType を追加するだけで機能拡張可能

ノードベースルーティングは、**「自在な音作り」**というToneDockのコンセプトを
技術的に実現するための中核アーキテクチャである。
