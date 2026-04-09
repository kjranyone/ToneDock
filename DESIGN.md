# ToneDock Design Guide

ToneDock は「汎用Webアプリ風のUI」ではなく、「プロフェッショナルなデジタルギター機材を操作する専用アプリ」の見た目を目指す。

## Design Direction

- キーワードは `digital rack`, `studio hardware`, `guitar processor`, `dedicated machine UI`
- 雰囲気は DAW よりも、Fractal / Kemper / BOSS / 高級ラックFX / アンプモデラー寄り
- フラットで白い業務アプリ風にはしない
- サイバーパンクやゲームUIにも寄せない
- 目的は「重厚感」「信頼感」「機材感」「操作の明瞭さ」

## Reference Images

現在の方向性は次の2枚を基準にする。

- [docs/Gemini_Generated_Image_5mamwr5mamwr5mam.png](/C:/lib/github/kjranyone/ToneDock/docs/Gemini_Generated_Image_5mamwr5mamwr5mam.png)
  Rack View の基準。金属パネル、縦積みモジュール、小型ディスプレイ、ノブ、背景写真の扱いを参考にする。
- [docs/Gemini_Generated_Image_50fy6u50fy6u50fy.png](/C:/lib/github/kjranyone/ToneDock/docs/Gemini_Generated_Image_50fy6u50fy6u50fy.png)
  Node View の基準。暗いキャンバス、機材風ノード、発光ポート、光る接続線、右側インスペクタを参考にする。

## Core Principles

- 既存の操作導線は保つ。見た目の強化で機能の場所を変えすぎない。
- 1画面の情報密度は高くてよいが、優先度は必ず見た目で分かるようにする。
- 主要操作は「機材のボタン」、パラメータは「フロントパネルの調整子」として見せる。
- 背景は単なる無地ではなく、控えめな質感や空気感を持たせる。
- 立体感は使うが、過剰な装飾で読みにくくしない。
- 紫アクセントは使ってよいが、主役は暗い金属とアンバー/グリーン/Cyan 系の機材色。

## Color and Material

- ベースカラーは黒に近いグラファイト、チャコール、ガンメタル
- 明部は白ではなく、鈍いシルバーや淡いグレーで表現する
- アクセントは次を使い分ける
  - 紫: 選択、強調、ハイエンド感
  - 緑: active / loaded / play
  - 赤: record / error / stop 系
  - 黄〜アンバー: bypass / warning / mono port / analog feel
  - Cyan: stereo / signal / output
- 質感は `brushed metal`, `smoked glass`, `OLED/LCD`, `soft LED glow`

## Typography

- 情報ラベルは小さく、機材ラベル風に扱う
- セクション名は短く、英大文字が基本
- 本文よりラベル、値、状態を優先して見せる
- 文字サイズの役割を混ぜない
  - 18-20px: ブランド/画面の主タイトル
  - 12-14px: モジュール名、選択中の見出し
  - 9-11px: セクションラベル、補助情報、メーターラベル

## Layout Rules

### Top Bar

- 単なる横並びにしない
- `FILE`, `ENGINE`, `VIEW`, `EDIT`, `STATUS` のセクションに分ける
- 強操作は独立した塊として見せる
- 右端には短いステータスメッセージを置く
- バー自体は金属パネル風の細い横帯にする

### Bottom Bar

- `METRONOME`, `LOOPER`, `STATUS` の塊に分ける
- 録音や再生系のボタンは色つきの状態表示を持たせる
- 下部バーは transport として見える必要がある

### Main Area

- メインパネルに白い既定枠を出さない
- 余白は「広い白地」ではなく、「装置の内部空間」として扱う
- サイドパネルは主領域を邪魔しない幅で固定する

## Rack View

Rack View は「プラグイン一覧」ではなく「デジタルラック機材の縦積みビュー」として扱う。

- 各 plugin slot は 1 台の rack unit / module として描く
- plugin 名は装置名、vendor/category は小さな補助ラベル
- loaded/enabled/bypassed 状態は LED や色で見せる
- `Enable`, `Bypass`, `Open GUI`, `Remove` はフロントパネル操作子として見せる
- 空状態でも「無地の空箱」ではなく、ラックにモジュールが入っていない感じを出す
- 背景はラックの内壁やスタジオ空気感を示す程度に留め、可読性は壊さない

### Rack View で避けること

- 普通のカードUIに見えること
- Web SaaS の一覧画面っぽさ
- 装飾のためだけの大きい写真で情報が埋もれること

## Node View

Node View は「抽象図」ではなく「信号処理モジュールの編集画面」として見せる。

- ノードは矩形カードではなく、小型DSPモジュールや専用ユニット風に描く
- ヘッダ部分は機材ラベル帯
- ポートはジャックや発光端子のように扱う
- 接続線はベジェでよいが、単色の線ではなく信号の存在感を持たせる
- 背景グリッドは主張しすぎず、技術図面や制御面の空気を出す
- 右側には `NODE INSPECTOR` を置き、選択中のノード情報やパラメータを表示する

### Node Types

次のノードは見分けやすくする。

- Audio In / Audio Out
- VST Plugin
- Gain
- Pan
- Mixer
- Splitter
- Wet/Dry
- Send / Return
- Metronome / Looper

見分け方は色、ポート、ヘッダラベル、アイコン風の差で出す。形をバラバラにしすぎない。

## Controls

- ノブは現在の円形ノブをベースにしてよいが、機材のつまみ感を強める
- トグルは単純なモバイル風スイッチではなく、パネル上の電源/有効スイッチに見せる
- ボタンは押下感と状態差を持たせる
- `Rec`, `Play`, `Overdub`, `Start Audio`, `Stop Audio` は色で意味を持たせる

## Meters

- 横メーターでもよいが、安っぽいプログレスバーに見せない
- セグメント式、暗い窓、細いラベル、LED 的な発色を優先する
- `INPUT`, `OUTPUT`, `MASTER OUT` は視線で探しやすい位置に置く

## Background Usage

- 背景画像を使う場合も、主UIの可読性を最優先する
- 背景は全面写真より、暗くぼかしたスタジオ風景や金属テクスチャが向く
- 操作面の直下に強い模様を置かない
- 背景は「雰囲気づけ」であり、主役ではない

## What To Avoid

- 白背景
- 平坦な業務アプリUI
- 典型的なSaaSダッシュボード
- 強すぎるネオン
- SF コックピット
- おもちゃっぽいペダル表現
- 角丸だけで成立した generic dark mode
- 情報より装飾が勝つ構図

## Implementation Notes

- `src/ui/theme.rs`
  色、面、ストローク、背景質感の基礎をここで揃える
- `src/app.rs`
  上下バーと Rack / Node 各ビューのレイアウトを調整する
- `src/ui/rack_view.rs`
  Rack View のユニット描画と選択状態を扱う
- `src/ui/node_editor.rs`
  Node View のノード、接続線、背景キャンバスを扱う
- `src/ui/meters.rs`
  メーターの質感を扱う
- `src/ui/controls.rs`
  ノブ、トグル、今後のボタン部品を扱う

## Practical Rule For Future Edits

新しいUIを足すときは、まず次のどれかに分類する。

- rack hardware
- signal routing hardware
- transport hardware
- inspector / utility panel

分類できない見た目は、大抵 ToneDock の方向から外れている。
