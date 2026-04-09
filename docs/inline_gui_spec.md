# Inline VST GUI Spec

Rack Mode における VST3 プラグイン GUI のインライン表示とフォールバック動作の仕様。

## 概要

ToneDock は Settings → `Inline plugin GUI inside Rack Mode` で VST3 プラグイン GUI を Rack View 内に埋め込み表示できる。デフォルトは separate window モード。

## Editor Mode

| Mode | Description |
|---|---|
| `SeparateWindow` | プラグインごとに独立したトップレベルウィンドウ |
| `Embedded` | Rack View 内の専用パネル領域に child HWND として埋め込み |

## Inline Mode ON の動作

1. ユーザーが Rack slot の "Open GUI" をクリック
2. `ToneDockApp::open_rack_editor()` が `inline_rack_editor_node` を設定
3. `show_rack_view()` 内の inline GUI パネル領域で `ensure_inline_rack_editor()` が呼ばれる
4. `PluginEditor::open_embedded_window()` を試行
   - **成功**: child HWND を Rack パネル内に配置、`set_embedded_bounds()` でリサイズ追従
   - **失敗**: 自動的に `PluginEditor::open_separate_window()` にフォールバック
     - status message: `Inline GUI failed, opened separate window: <plugin>`
     - `inline_rack_editor_node` は `None` にリセット

## State 整合ルール

- `inline_rack_editor_node` は常に「現在 inline 表示中のノード」を指す
- inline mode OFF 時は全エディタを close（`close_all_rack_editors()`）
- View mode 切替時（Rack → Node Editor）も全エディタを close
- `rack_plugin_editors` HashMap は inline/separate 両方のエディタを格納
- `is_open()` は HWND の有効性も検証するため、クラッシュ等でウィンドウが消失した場合は自動的に `false`

## Inline GUI パネル仕様

- Rack View メイン領域下部に配置
- パネル高さ: `preferred_size` ベース、220px〜520px にクランプ
- 幅: パネル利用可能幅（最低 320px）
- 背景: `Color32::from_rgb(10, 10, 12)` + `CornerRadius::same(10)`
- 閉じるボタンあり

## Smoke Test

実プラグインでの検証用に 2 つの ignored test を用意:

- `smoke_open_plugin_editor` — separate window の open/close
- `smoke_open_plugin_editor_embedded` — embedded の open/close/reopen

```powershell
$env:TEST_VST_EDITOR_PATH='C:\Program Files\Common Files\VST3\NeuralAmpModeler.vst3'
cargo test smoke_open_plugin_editor -- --ignored --nocapture
cargo test smoke_open_plugin_editor_embedded -- --ignored --nocapture
```

## 制約

- Windows のみサポート（HWND ベース）
- inline と separate の二重オープンは不可（同一ノードでは排他）
- VST プラグインロードの undo は未対応
