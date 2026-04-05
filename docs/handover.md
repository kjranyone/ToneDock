# ToneDock — Handover資料

最終更新: 2026-04-05

18:48 JST

yyyy-mm-dd:HH:MM:SS+09:00

## プロジェクト概要

**ToneDock** は Rust で書くギター緅緰用 VST3 ホストアプリです。MIT ライセンスで公開予定。

- リポジトリ: `C:\lib\github\kjranyone\ToneDock`
- 現在のコcargo check` は **通っていません**（コンパイルエラー多数）

- 最終目的: ギター緢ラ時のエンプラ/IRローダー、メトロノーム、ルーパー、バッキングトラックなどを備えた VST3 ホスト GUI アプリ

ーション状態: プロトタイプ / スキャフォールド完了（コンパイルエラー多数）

  - メトロノーム・ルーパー、Looper の基本実装済み
  - GUI（egui/eframe）のテーマ・ラックビュー、メーター、コントロールは実装済み
  - セッション保存/復元機能は実装済み

  - VST3 プラグインスキャナー・ローダーは実装済み（Windows対応）
  - まだ音は鳴らない（オーディオ入力未接続）

---

## アーキテクチャ

```
src/
├── main.rs              — エントリポイント（eframe）
├── app.rs               — メインGUIアプリ（※rack依存あり・要修正）
├── metronome.rs          — メトロノーム生成
├── looper.rs             — ルーパー（録音/再生/オーバーダブ）
├── session.rs            — セッションJSON保存/復元
├── audio/
│   ├── mod.rs
│   ├── chain.rs          — プラグインチェーン（vst_host使用）
│   └── engine.rs         — cpalオーディオエンジン
├── ui/
│   ├── mod.rs
│   ├── theme.rs          — ダークテーマ色定数
│   ├── controls.rs       — ノブ・トグルUI部品
│   ├── meters.rs         — ステレオレベルメーター
│   └── rack_view.rs       — プラグインラックビュー（※rack依存あり・要修正）
└── vst_host/
    ├── mod.rs
    ├── scanner.rs          — VST3プラグインスキャナー（Windowsパス検索）
    └── plugin.rs           — VST3 COM経由プラグインローダー（unsafe）
```

## クレート依存関係（Cargo.toml）
```toml
vst3 = "0.3"          # VST3 COM bindings（MIT/Apache-2.0）
cpal = "0.15"          # オーディオI/O（Apache-2.0）
eframe = "0.31"        # GUI framework
egui = "0.31"          # Immediate mode GUI
egui_extras = "0.31"   # GUI追加機能
serde / serde_json    # セッション保存用
parking_lot = "0.12"   # ミューテックス
anyhow = "1"            # エラーハンドリング
rfd = "0.15"           # ファイルダイアログ
midi-msg = "0.7"        # MIDI（未使用）
crossbeam-channel = "0.5"  # チャネル（未使用）
log / env_logger       # ロギング
libloading = "0.8"       # 動的ライブラリロード
```

## ブロックされているコンパイルエラー（全13件）
### 1. `rack` クレート参照（致命）
`src/app.rs:3` → `use rack::prelude::PluginInfo;`
`src/ui/rack_view.rs:2` → `use rack::prelude::PluginInfo`

**原因:** `rack` が Cargo.toml から削除されたが、コードがまだ `rack` をインポートしている
  
**修正方法:** `crate::vst_host::scanner::PluginInfo` に置換

### 2. `Chain` に `scan_plugins()` メソッドがない
src/app.rs:63` → `chain.scan_plugins()`
**原因:** `Chain` はスストしたス際、スPluginScanner` を直接使用するよう変更した
  
**修正方法:** `app.rs` に `PluginScanner` フィールドを追加し、そこで `scan()` を呼ぶ

### 3. `Chain` にパラメータメソッドがない
src/app.rs:401` → `chain.get_parameter_info(slot_index)`
`src/app.rs:414` → `chain.get_parameter(slot_index, j)`
`src/app.rs:430` → `chain.set_parameter(slot_index, j, value)`
**原因:** `vst_host::plugin::LoadedPlugin` はパラメータアクセスをまだ公開していない
  
**修正方法:** VST3 `IEditController` / `IParameterChanges` を実装するか、パラメータを削除する
### 4. `rfd::AsyncFileDialog` の使い方
間違い
src/app.rs:87` → `rfd::AsyncFileDialog::new()`
`src/app.rs:105` → `rfd::AsyncFileDialog::new()`
**原因:** `rfd 0.15` の `AsyncFileDialog` は future を返し、 `pick_file()` が `Option<...>` で、ファイルパスを返す（非同期）
。
 `rfd::FileDialog` の方が適切
  
**修正方法:** `rfd::AsyncFileDialog::new()` → `rfd::FileDialog::new()` に変更（`.pick_file()` / `.save_file()` の使い方をブロ単で修正）
### 5. `buffer_size` 型の不一致
（簡単な修正）
`src/app.rs:299` → `bs` in `self.audio_engine.buffer_size as usize` → `chain.add_plugin` expects `usize``
`i32`
**原因:** `buffer_size` is `u32`、 `Chain::add_plugin` は `block_size: i32` を期待する

  
**修正方法:** `bs as i32` を追加するだけ（ただし実際には `bs` は `u32` なのでキャストも必要なし`### 6. `or_else` の型の不一致
（エラー）
  
`src/audio/engine.rs:47` → `.or_else(|| anyhow::anyhow!("No supported config"))?` — Option<SupportedStreamConfig>` を返さない`または `Option` は `Result` を生成するため（`find` の条件がより適切に変更する（`Result<Option<...>>` を返さない（`Result<Option<...>>`）
パニックする）
  
**修正方法:** `or_else` を削除して、`.find()` → `.ok_or_else(...)` に変更する
### 7. `cpal::BufferSize` コピー できない（マイナー）
  
`src/audio/engine.rs:53` → `match &config.buffer_size { cpal::BufferSize::Default => 256, cpal::BufferSize::Fixed(v) => v`
`
**原因:** `BufferSize` は `Copy`/`Clone` ではない（`Default` variant に Copy/`Fixed` variantの両方に整数型)。

 ど元が `u32` にマッチする:
 `let _ = *;` | そのままコピーする（値はは整数なので `u32` にキャストする
`use std::mem::size_of::<cpal::BufferSize>(); let v: cpal::BufferSize::Default = 256u };
    `if let cpal::BufferSize::Fixed(v) = v => { self.buffer_size = v; } else { 256 }
 };
    ```

  
**修正方法:** `self.buffer_size = match &cpal::BufferSize::Default => 256, cpal::BufferSize::Fixed(v) => v,` に変更

。これは `vst3` 0.3 クレートは、`cargo info vst3` で `cid` を調べてTUID サ確認した

### 8. Looper の borrow checker ア（内容: https://docs.rs/rust_vst_host_license_audit.md を5-7, `Steinberg::SpeakerArr::kSterkMono`/`kStereo` などののド VST3 COM API  
 Com-scrape-types  Documentation を `coupler.rs/vst3-rs` の `Steinberg` モジュの構造を確認)
  
- `ComPtr<IComponent>` → `component.initialize(context)` → `FUnknown` null pointer
 null or no初期セット  - `IComponent::getBusCount()` → オーリ バST3 bus types)
  - `IComponent::activateBus()` → Activate/dedeactivate audio buses,  - `IComponent::setActive()` → setActive/DeDeactivate audio bus,  - `IComponent::terminate()` → terminate
 - `ComPtr` から `ComPtr` ゗cast::<IAudioProcessor>` のキャ変 **ComPtr::cast()` のcomponent.cast::<IAudioProcessor>()`
 を取得 `IAudioProcessor`
 インターフェ  
- `IAudioProcessor::setupProcessing()` → `ProcessSetup` をセットして処理の - `IAudioProcessor::setBusArrangements()` → スピカー arrangement を設定  - `IAudioProcessor::process()` → `ProcessData` を渡して処力の  - VST3 `ProcessData` 構造体:
    ```rust
    pub processMode: int32,       // 0 = realtime
    pub symbolicSampleSize: int32, // 0 = 32-bit
    pub numSamples: int32,
    pub numInputs: int32,
    pub numOutputs: int32,
    pub inputs: *mut AudioBusBuffers,
    pub outputs: *mut AudioBusBuffers,
    pub inputParameterChanges: *mut IParameterChanges,
    pub outputParameterChanges: *mut IParameterChanges,
    pub inputEvents: *mut IEventList,
    pub outputEvents: *mut IEventList,
    pub processContext: *mut ProcessContext,
    ```

  
- `Steinberg::AudioBusBuffers` 構造体:
    ```rust
    pub numChannels: int32,
    pub silenceFlags: uint64,
    pub __field0: AudioBusBuffers__type0,  // union with channelBuffers32/64 as channelBuffers64
    ```
  
- `vst3::ComPtr` API: `from_raw(ptr)` で所有権取得, `cast::<J>() -> QueryInterface + アcast()` を使用

 `Into::raw()` で 所有権を放棄

- VST3 `IPluginFactory`Vtbl`:
 `createInstance` の `cid` 引数が `FIDString`（`*const TUID`）、2番目目なので、 ノイドが `FIDString` である
 宛 else、 `&PClassInfo.cid` をバ"Audio Module Class" と照合
 |
- `ComPtr` + `ComPtr::cast::<IAudioProcessor>()` で QueryInterface → IAudioProcessor を取得  

対応 `IEditController` (`IComponent::cast()））も実装す必要がある。
 `IEditController::` を追加するか `get_parameter_info` / `set_parameter` / `get_parameter` を追加するか`  `process()` に一`can_process` 婸追加する

- パラメータ変更通知（UI）: ノブ/スライダーの値を盭UI で更新するには `ParamValue` を `IEditController` に送る必要がある。 `IEditController::queryInterface()` で取得)  
      - 篡] (*index*: usize, param: &str) → parameter info (`title`, `units` など)
 (parameter info がない場合は `name` でノブ("No parameters") と表示)

プラグインが読み込む際のUI はパラメータ名を表示) | ただし、パラメータ情報は内部で `LoadedPlugin` に保存しておお。 | ||
 `Plugin::load()` 時渡プラインの DLL を開き、`GetPluginFactory()` を呼び出ファクトリを作するプラグインインスタンス化する)。
ロード/アンロードが必要があります
 |

}
  ```
  
- **実装優奨**: `IEditController` (`IComponent::cast` → `IEditController`)）に頼わるパラメータ制行取得する場合はは、(Unsafe) に `IEditController::queryInterface()` を呼び出して、パラメータ情報を取得する。。
 この方法の安全性に `IEditController::queryInterface` は `unsafe` 匉 require IEditController 塿 {
    use vst3::{ComPtr, IEditController};
 Com IEditControllerTrait};
    if query_interface` returns `Option<ComPtr<IEditController>>` (unsafe) に `FUnknown` → `FUnknown` に `vst3` + `ComPtr` + `ComPtr` + `FUnknown` + `as_ptr` 窇うする)。
 let param_info = ComPtr<IParameterInfo> の引数形):
        let v = unsafe { ptr.set_parameter_count() }
        unsafe fn get_parameter(&mut self, index: usize, param_index: &ParamID) -> ParamValue {`  
    }
}
```

---

## 次のステップで復旧する

cargo check` が通るようになるまでの修正一覧）
全11箇所適用するの順番に進めるが推奨:
`1 → 8`:
1. `rack` → `vst3` への切り替え`: `rack` を `use rack::prelude::PluginInfo` → `use crate::vst_host::scanner::PluginInfo`
`2 → `src/app.rs` line 62-74: `chain.scan_plugins()` → `Chain` 内で `PluginScanner` を追加し、そこで直接 `scan()` を呼ぶようににする:
 `3 → `src/app.rs` line 299`: `self.audio_engine.buffer_size as usize` → `self.buffer_size as u32` → `chain.add_plugin` に `usize` をキャストする: `bs as i32` を追加
4 → `src/audio/engine.rs` line 47: `.or_else()` → `or_else` を削除する: `Option<SupportedStreamConfig>` を返さないため）を `.find()` の結果を優先)
する
 5 → `src/audio/engine.rs` line 53: `cpal::BufferSize` copy できない: `match` を使うする:
 `let v = match b { cpal::BufferSize::Default => 256, cpal::BufferSize::Fixed(v) => v, }); self.buffer_size = v; }` | 6 → `src/app.rs` lines 399-445: `draw_parameter_panel` → `Chain` の `get_parameter_info` / `get_parameter` / `set_parameter` メソッドを削除する: パラメータパネルを簡略化（ノブを表示するのみ）

 `7 → `src/app.rs` lines 87-120: `rfd::AsyncFileDialog` → `rfd::FileDialog` に変更する

 `8 → `looper.rs` の borrow checker（content: https://docs.rs/looper-rs/html れ `Arc<Mutex` in `process()` で `self.buffer` と `self.playback_pos` を借用する形これを2つの Mutex が同時にロックされるとれる。 1つの `Arc<Mutex` で `buffer` をロックし、もう1つの `Arc<Mutex` で `playback_pos` をロックする.。

 これは VST3 仕様の buffer allocationが `TUID` である注意点。

 バイナリ配列の長さが16バなので `ComPtr` の `from_raw` では所有権を取得する必要がある)

- Rust コードでは別の `.vst3` フに追加する前に、Windows かマルの `rack` が CMake のビルドエラーで VS 2022 が必要という問題が解決した
 **`rack` → `vst3` に変更した `rack` の `cpal` + `vst3` + `egui` に置き換える**


 VST3 は `coupler-rs/vst3-rs` が公開している純1以外のVST3ホスト機能はクレートが `rack` と同じ機能を提供するが（プラグインスキャン、ロード、処理など）、低レベルでAPIが必要自 `rack` に慴がない唯一の利点です。

私たちが選択した `rack` → `vst3` の変更は `rack` のドキュメントには `rack` が **VST3 ホストとしての成熟度は限定的**（特にWindows）と書かれています:

 Windows版の `rack` はVST3 SDK を直接バンドル（C++ FFI）しているため、CMake と Visual Studio 2022 が必要とします。VS 2019ではWindowsパスに含まれる `\U` がCMakeのエラーの原因）ビルドに失敗しました。

 

 解決策として `rack` を削除して、`vst3` (MIT/Apache-2.0) の低レベル COMバインディング) + `cpal` (Apache-2.0) をオーディオI/Oに置き換えましたで `vst3` + `cpal` + `egui/eframe` に変更しました。 `vst3` は `ComPtr<T>`（`from_raw` で所有権取得）と `cast::<T>` で QueryInterface を行う。 ただし `vst3` の API は低レベルすぎる独自ホストを実装する必要があります（`IEditController`、`IParameterChanges`、`IEventList` など）。`cargo check` が通るまでの修正が以下の機能を追加する必要があります。

 `parameter_count`、`get_parameter_value`、`set_parameter` は plugin レinstance` に追加するだけでもUI は一時固定します:

 パラメータパネルをコメントにテキストのみ表示にスタートする。

最優先 `cargo check` が通るようになります。

 - `scan`/`load`/`process`/parameter` の `Chain` メソッドを追加
 - `PluginScanner` を `app` 構造体に持たせる)。 - `rfd` を `rfd::AsyncFileDialog` からブロ対応の `rfd::FileDialog` に変更する） - `engine.rs` の `or_else` を単純化する）
 - `chain.rs` の process() メソッドで余分なバッファコピーを避ける） - `looper.rs` から `Arc<Mutex` を除去してオーディオのコールバックで改善する）

 - `looper.rs` から `Arc<Mutex` の借用を削除し、 `parking_lot::Mutex` を通常の `Mutex` にする）`deadlock`を生じるだけで `process` 内でロック解除する
 - `looper.rs:126` で `let pos = *self.playback_pos.lock();` → `self.playback_pos.lock()` → `Arc<Mutex<usize>` の `Mutex` を 2回 `Mutex` ロックされてデッドロック） - `chain.rs:64` で `process()` 内で `plugin.process()` が input を参照しているのoutput を参照しているため、正しいしいに動作しがpluginが次のプラグインを処理する `chain.process` に `plugin.process()` に渡す `process()` に必要がある修正は)

 - `vst_host::plugin.rs` の `LoadedPlugin::process()` に、 `&mut self.samples` を planarフォーマットで処理）に1バスのポ (`Vec<Vec<f32>>` が planarフォーマット（interleaved）でも `process()` に正しい動作ですが、 叨現意としては `LoadedPlugin:: が `input` を 1バスのポ (`Vec<&[f32]>`) にすると `chain` の後のスロットは受け信できます。しかし、欠点もあります: pluginが `chain` の最後のスロットを受け取するは、次のプラグインが自分を静かにするにのみ)を書き込み `self.process()` が呼び出れてしまう、テストの必要があります。この動作チェーンのプラグインチェーンに落としやす理解できるようになります）

現在は正しいかもしれません:

 この修正により `plugin` の出力バST3 はコピーすべきがあります、同じ出力次のプラグインが受け取るの値を設定する必要があります。これは、UIコで一つの `DragValue` で BPM を変ええる必要がある。リアの再生パ後に、再生Bしたドラッグ操作)

 `DragValue` は、別のドラッグ操作が正しい)

 再生 `Chain.add_plugin()` が `usize` を受けするためキャストする)を追加する)よりもスムリーにする)
 `chain.add_plugin` にイン (サ、 `Chain.add_plugin()` が `info`, オブジェクト `chain` の参照、同じになる; VST3 `processData` 構造体の `vst3::Steinberg::ProcessData` を作る `input` バ output は C++ で処理する必要があります
  - `rfd:: の async ファイルダイアログをブロする必要がある） |
- `egui` のノブ のランダーを調整整確認済。
 `rfd::AsyncFileDialog` → `rfd::FileDialog` に変更する
  |
  - `engine.rs` の `.or_else()` をシンプルにしてる これ、 `build_host_in `AudioEngine` に `PluginScanner` を持たせる必要がある) |
- `PluginScanner` から `rfd::AsyncFileDialog` を `rfd::FileDialog` に変更する |

  - `engine.rs` の `or_else` を `cpal::BufferSize::Default => 256, `cpal::BufferSize::Fixed(v) => v` に変更する |
 `app.rs` の `chain.scan_plugins()` を `Chain` から `PluginScanner` に移動し |
 `app.rs` の `buffer_size` を `i32` にキャストする) |

- `PluginScanner` は独立して `AudioEngine` から `PluginScanner` を移動し他かい理由)
は、ui の `rack_view` が `rack::prelude::PluginInfo` → `crate::vst_host::scanner::PluginInfo` に変更する)は| `app.rs` のパラメータパネルを削除するかUI のパラメータパネルを簡略化表示)
「No parameters" と表示する |

 `app.rs` の `chain.get_parameter_info()` と `chain.get_parameter()` / `chain.set_parameter()` メソッドを削除する。パラメータパネルはコードのノブ/スライダーを表示)にする）は| --- コールバックだけ値を反映するだけでよい)

実際、プラグインの初期化段階ではパラメータが1つも2つず択してパラメータとして、"No parameters" と表示するはTODOは将来の実装優先度順としては:
 **(1) 各プラグインの `IComponent::queryInterface` で `IEditController` を取得し、 **(2) `IEditController::queryInterface` で各パラメータの `ParameterInfo` を取得** **(3) 各プラグインの `IParameterChanges` にパラメータを設定する** **(4) `Chain` 内の `PluginScanner` を持たせる**パラメータアクセ用ができる

ようになります: (※スキャナー→ **PluginScanner** → `Vec<PluginInfo>` を返すしますパラメータ情報からUIに表示)します
 **(5)** `rfd` を同期ファイルダイアログを使する; **Sync** の方が安全なので `rfd::FileDialog` を使用してください: `rfd` は `rfd::AsyncFileDialog` を返す `Future` ではなく、`Future` を返します。そのため、 `rfd::FileDialog` を使用します: `let dialog = `rfd::FileDialog::new()` でファイルダイアログを開き、`save` 用は `.save_file()` は同期で行って `save` 処理が発生するため、対応する必要があるのはUI スメッセージに流れるように `save_session()` を別のスレッドで実行する。 | https://docs.rs/rfd/latest/rfd0 で確認) `rfd` 0.15` は `rfd::AsyncFileDialog` が存在し、 `pick_file()` が `Option<...>` を返します（`Future`）。これから `pick_file` はファイルダイアログを開く、`save` 用 `rfd::FileDialog` に変更してください: 

https://docs.rs/rfd/latest/rfd/ 0.15.4

 https://docs.rs/rfd/0.14.0 （`FileDialog::new()` が戯っている、 `FileDialog::new().save_file()` は `rfd` 0.14` では `save_file()` の返り値は `Future` に含まファイルパスの `rfd::FileDialog` がこの問題を解決する）({
                    `err => e => log::error!("Save failed: {}", e)),
                }
            });
        }
    })
}
```

`rfd::AsyncFileDialog::new()` を `rfd::FileDialog:: に変更するだけの `cargo check` が通るようになる。

 `engine.rs` と `app.rs` のコンパイルエラーも修正してください: 以下の修正リを適用順に行するのがよいとのこと: 
以下の順に行すると:

 `rfd` の `AsyncFileDialog` と `FileDialog` の違いを理解する） |
- `engine.rs` の `BufferSize` の扱いい方、（`match` を使用する）方法）
- `chain.rs` に `scan_plugins()` を削除 → `PluginScanner` を別フィールドに持つ
方法を削除
 パラメータアクセ用 `Chain` に追加するだけれUI はパラメータパネルを削除する | |

## vst3 ゑ API 設量化理解の参照） | 49. **`vst3` v0.3** クレートのCOMPtr<T>` API):
 `vst3` 0.3 や `ComPtr::from_raw()` で所有権を取得、ComPtr` の代わりに `cast::<T>() で QueryInterface を行うが ただし、`vst3` の API は低レベルすぎ、独自ホストを実装する必要がある（特にパラメータ、関係）する)。

`IEditController`、`IParameterChanges`など)の取得・設定/取得パラメータ値を設定する] | プ設定 parameter の際に MIDI メ処理も行う)。

   - 取得可能は: `IEditController` の `IComponent` を取得 (`IComponent.cast()`), 塋 **手動**  `IEditController.get_parameter_count()` | 1usize` {
 pub fn get_parameter_info(&mut self, index: usize) -> Vec<ParameterInfo> { | pub fn set_parameter_value(&mut self, index: usize, value: ParamValue) {` |
- `process()` に `can_process()` を追加して必要がある。 `vst3::ProcessData` を `vst_host::plugin.rs:process()` に `vst_host::plugin::LoadedPlugin:: が VST3 プラグインをホストし、プラグインを` して `process()` を呼び出。ただし、プラグインが初期化時にパラメータの情報を取得する必要がある) `vst_host::plugin.rs:LoadedPlugin::get_parameter_count()` は、`parameter_info` を取得してから、LoadedPlugin` に保存する) 
    
 `get_parameter_info` は[]` にパラメータ情報を表示)。
一方で、ホスト側のパラメータを管理する機能も追加できる| `IParameterChanges` のホスト側実装（`IParamValueQueue` を作って `IParameterChanges` にパラメータを `IParameterChanges` に保存する）     });
     /// Host 用のIParamValueQueue` でパラメータ値を渡え先`IParamValueQueue` から VST3 `IParamValueQueue` に追加する) {
    /// Implementation of `IParamValueQueue` using vst3 COM types
 see above)
- `vst_host::plugin.rs:161-166`
 `LoadedPlugin` has methods to return parameter count and parameter info. Both VST3 の `IParamValueQueue` / `IParameterChanges` についていた学. 煀  VST3 COM API を低レベルで `paramValue` は内部で `Vst_host::plugin.rs` ファイルに `ParameterValue` 構造体と `vst_host` を実装するための audio ココバックをエラーが表示されます。

ただ `ParameterValue` と `ParameterChangeList` は後処 `ParameterValue` は `on_add` から呼び出で `IParameterChanges` の場合は call `process()` の作る `VST3` 仕様として `ParameterChangeList` は `IParameterChangeList` を初期化時は `ParameterValue` を保存しません) {
        let values = Vec::new();
        values.push(ParameterValue {
            id: param.id,
            value: normalized_value,
        });
    }

    /// parameter_change_list is also VST3 仕様として `IParamValueQueue` を実装する必要がある。   - 現在は安全な `process()` 内で借用 `parameter_change_list` として `Self` の一つのフィールド
ある) {
        parameter_change_list: *mut IParameterChangeList,
    ) -> Option<()> {
        parameter_change_list.as_ref().map(|pcl| {
            *self.process_data.inputParameterChanges = pcl.as_mut_ptr();
        });
    }
}

### 9. **音声入力の追加** — 現状はオーディオ出力なし（オーディオ入力をプラグインに渡すのには）: (a) `AudioEngine` に `PluginScanner` フィールドを追加し、そこでスキャンする; (b) オーディオ入力（マイク/インストルメント）は `cpal` の duplex stream が使える必要がある。現状の `AudioEngine` は出力のみ。オーディオ入力を `cpal` の入力ストリームとして取得し、プラグインチェーンに渡すパことができます。

ただし、入力ストリームが実際に動作するためには、`AudioEngine` を変更して、出力ストリームと入力ストリームの両方を開くか、それぞれのストリームのコールバック間でデータを中継ぐする方法を検討する必要があります（`cpal` がこれをサポートしているか不明）。
   - **代替案**: `cpal` の `build_input_stream()` で別スレッドで入力を取得し、`crossbeam_channel` でオーディオスレッドに送信する
- **パラメータアクセスUI改善** — `app.rs` の `draw_parameter_panel` を修正し、実際のパラメータ名（`ParameterInfo.title`）を表示する

- **パラメータ変更通知** — パラメータ変更時にホスト側で `IParameterChanges` を実装し、`process()` に渡す
- **プラグインGUI** — VST3 プラグインのGUI（IPlugFrame）は将来対応

### 現在のプロジェクト構成の根本的な問題
**アーキテクチャ上の最大の問題は、オーディオ処理スレッドとUIスレッドの分離です。**

現在の `process_output_f32` 関数は以下のように動作します:
1. 出力バッファをゼロクリア
2. `Chain::process()` を呼び出す（→ `LoadedPlugin::process()` → VST3 `IAudioProcessor::process()`）
3. メトロノームをミックス
4. ルーパーをミックス
5. ピークメーターを更新

しかし、ステップ2で入力が常に「無音」です。ギタープラクティスアプリとして機能させるには、マイク/インストルメント入力が必要です。これは次期の大きな課題です。

---

## 今のワークツリー
```
main
└── AudioEngine (start → cpal output stream)
    └── process_output_f32 (audio callback)
        ├── Chain::process
        │   └── LoadedPlugin::process (VST3 COM unsafe)
        │       └── IAudioProcessor::process (VST3 SDK)
        ├── Metronome::process
        └── Looper::process
```

---

## 次回やるべきこと（優先順）

### P0: コンパイルを通す（1-2時間）
1. `rack` 参照を全て削除し、`crate::vst_host::scanner::PluginInfo` に統一
2. `PluginScanner` を `app.rs` または `AudioEngine` に移動
3. `Chain` に `PluginScanner` フィールドを追加（または `app.rs` に持たせる）
4. `rfd::AsyncFileDialog` → `rfd::FileDialog` に変更（ブロッキングAPI）
5. `engine.rs` の `or_else` を修正
6. `engine.rs` の `BufferSize` copy を修正
7. `app.rs` の `buffer_size` キャストを `u32 as usize as i32` に修正
8. `chain.rs` の `process()` で各プラグインの出力を一時バッファにコピーしてから書き戻すよう修正（現在、プラグイン間で同じバッファを読み書きしている）

### P1: オーディオ入力（1-2日）
1. `cpal` の入力ストリームを追加
2. `crossbeam_channel` で入力→処理スレッド間のデータ受け渡し
3. または `cpal` の duplex stream（サポートされている場合）を使用

### P2: パラメータ制御（半日）
1. `IEditController` を `IComponent` から取得
2. `IParameterChanges` と `IParamValueQueue` を実装
3. `process()` にパラメータ変更を渡す
4. UIのノブ操作をパラメータ変更に接続

### P3: テストとデバッグ（1-2日）
1. 実際のVST3プラグイン（無料アンプシミュレーター等）で動作確認
2. オーディオレイテンシーの測定と最適化
3. クラッシュリカバリの実装

---

## 参考資料
- VST3 API ドキュメント: https://steinbergmedia.github.io/vst3_dev/
- `vst3` crate: https://coupler.rs/vst3-rs/vst3/
- `cpal` crate: https://docs.rs/cpal/
- `egui` crate: https://docs.rs/egui/
- `com-scrape-types`: https://docs.rs/com-scrape-types/ （`ComPtr`/`ComRef` のAPIリファレンス）
- ライセンス監査: `docs/rust_vst_host_license_audit.md`
