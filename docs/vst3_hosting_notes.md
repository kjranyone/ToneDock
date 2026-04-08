# VST3 Hosting Notes

ToneDock は VST3 を単にロードして `IPlugView::attached()` を呼ぶだけでは動かない。とくに Windows の iPlug2 系プラグインでは、processor/controller 初期化と host object 実装が不足すると GUI open でクラッシュしやすい。

## Required host behavior

### 1. Split controller を正規手順で扱う

- まず `IComponent` から `IEditController` を query する
- 取れない場合は `getControllerClassId()` を呼ぶ
- factory から controller を生成する
- controller に `initialize()` を呼ぶ
- component/controller が両方 `IConnectionPoint` を持つなら相互に `connect()` する
- editor open 前に component `getState()` -> controller `setComponentState()` を行う

これをやらないと `createView()` 自体は通っても、内部状態が不足したまま UI 初期化に入りやすい。

### 2. `IHostApplication::createInstance()` を実装する

ToneDock 側では少なくとも次を返せる必要がある。

- `IMessage`
- `IAttributeList`
- `IBStream`

一部プラグインは UI 初期化時に host message を送る。ここを未実装のまま `null` を返すと、ホスト依存のコードパスで壊れる。

### 3. Windows では `InitDll()` / `ExitDll()` を呼ぶ

Windows の iPlug2 VST3 は exported `InitDll()` で `gHINSTANCE` を初期化する。これを呼ばずに `GetPluginFactory()` だけ使うと、埋め込み resource のロードに失敗する。

今回実際に起きた症状:

- `NeuralAmpModeler.vst3` の `createView()` は成功
- `isPlatformTypeSupported("HWND")` も成功
- `IPlugView::attached()` で SEH `0xC0000094`

原因は host HWND ではなく、`InitDll()` 未実行で plugin 側の GUI resource 初期化が壊れていたことだった。

## Editor hosting on Windows

- attach 前に `setFrame()` を呼ぶ
- plugin には owner 直下の child HWND を渡す
- close 時は `removed()` の前に `setFrame(nullptr)` を呼ぶ
- plugin 呼び出しは `seh_wrapper.c` 経由で囲う

`IPlugView::attached()` は plugin 側の Win32 例外をそのまま飛ばすことがあるため、保護なしで直接呼ばない。

## Verification workflow

通常確認:

```sh
cargo check
cargo test
```

実プラグイン editor 確認:

```sh
$env:TEST_VST_EDITOR_PATH='C:\Program Files\Common Files\VST3\NeuralAmpModeler.vst3'
cargo test smoke_open_plugin_editor -- --ignored --nocapture
```

この smoke test は実プラグインをロードし、`setup_processing()` のあと editor を開いて短時間維持し、close まで確認する。

## References

今回の修正判断で直接参照した一次情報と、今後まず当たるべき資料。

- Steinberg `IEditController`
  https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IEditController.html
- Steinberg `IHostApplication`
  https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IHostApplication.html
- Steinberg hosting example `plugprovider.cpp`
  https://github.com/steinbergmedia/vst3_public_sdk/blob/master/source/vst/hosting/plugprovider.cpp
- Steinberg host-side helper implementation `hostclasses.cpp`
  https://github.com/steinbergmedia/vst3_public_sdk/blob/master/source/vst/hosting/hostclasses.cpp
- iPlug2 Windows module initialization and `gHINSTANCE`
  https://github.com/iPlug2/iPlug2/blob/master/IPlug/IPlug_include_in_plug_src.h
- NeuralAmpModeler `OnUIOpen()` と delegate message 送信
  https://github.com/sdatkinson/NeuralAmpModelerPlugin/blob/main/NeuralAmpModeler/NeuralAmpModeler.cpp

## How these references map to ToneDock

- `IEditController` docs では `setComponentState()` と `createView()` が `[UI-thread & Connected]` 前提になっている。ToneDock でも editor open 前に controller 接続と state 同期が必要。
- Steinberg の `plugprovider.cpp` は single-component を試し、だめなら `getControllerClassId()` から separate controller を生成する流れになっている。ToneDock もこの順に寄せるべき。
- `hostclasses.cpp` は `IHostApplication::createInstance()` で `IMessage` と `IAttributeList` を返している。ToneDock でも host object 未実装のままにしない。
- iPlug2 側の module init 実装を見ると、Windows では `InitModule()` で `gHINSTANCE` を初期化している。ToneDock では VST3 exported `InitDll()` / `ExitDll()` を明示的に扱う必要がある。
- NeuralAmpModeler 側は `OnUIOpen()` で `SendControlMsgFromDelegate(...)` を使う。UI open 時点で host message 系が使われる前提なので、controller/host object 実装が欠けていると壊れやすい。
