# W10Wheel.rs

W10Wheel.rs is the Mouse Wheel Simulator for Windows 10 / 11, the Rust version of W10Wheel ([Java](https://github.com/ykon/w10wheel) and [.NET](https://github.com/ykon/w10wheel.net) versions): the same features as their 2.10.0, sharing their settings files.

## Supported

Windows 10 / Windows 11 (x64). A single `W10Wheel.exe`: no runtime to install.

### Scroll inactive windows

Enable the Windows option to scroll inactive windows when hovering over them.
- Windows 11: Settings > Bluetooth & devices > Mouse > "Scroll inactive windows when hovering over them"
- Windows 10: Settings > Devices > Mouse > "Scroll inactive windows when I hover over them"

## Download

1. Download `W10Wheel.rs-<version>.zip` from [Releases](https://github.com/ykon/w10wheel.rs/releases). `W10Wheel.rs-<version>.zip.sha256` has its SHA-256 hash.
2. Extract it to any folder and run `W10Wheel.exe`.

`W10Wheel.exe` is not code-signed, so Windows SmartScreen may show "Windows protected your PC" when you run it for the first time.
If you downloaded it from the Releases page above, click "More info" and then "Run anyway".

## Usage

Right-click the tray icon and choose Settings... to open the settings dialog: a change is applied and saved at once. Double-click the tray icon (or choose Pause) to pause W10Wheel.

The settings files (`.W10Wheel*.properties` and `.W10Wheel*.exclusions` in your user folder) are shared with the Java and .NET versions. Only one W10Wheel runs at a time: exit the running one first.

Commands for the running W10Wheel (also of the Java and .NET versions): `W10Wheel.exe --sendExit`, `--sendPassMode [true | false]`, `--sendReloadProp` and `--sendInitState`.
See `Readme.ja.txt` in the zip for the details (in Japanese).
The [Wiki of the .NET version](https://github.com/ykon/w10wheel.net/wiki) may also help: it describes the .NET version, which works the same in most ways, though not every detail may match.

## Build

Requires Rust (stable, the MSVC toolchain) and the Windows SDK (`rc.exe`).
```
cargo build --release --locked
```
`target\release\W10Wheel.exe` is built. To build the distribution zip and its SHA-256 (`artifacts\W10Wheel.rs-<version>.zip`, `.zip.sha256`), with PowerShell 7:
```
pwsh build/package.ps1
```

## License

The MIT License

---

以下は日本語の開発者向けの説明です。利用者向けの説明は配布 zip の `Readme.ja.txt`（`dist/Readme.ja.txt`）にあります。

## ビルド

Rust（stable、MSVC ツールチェーン）と Windows SDK（`rc.exe`）が必要です。

```powershell
cargo build --release --locked
```

クレートを取得済みの環境では `--offline` を付けてもビルドできます。

単一の GUI バイナリ `.\target\release\W10Wheel.exe` が生成されます。C ランタイムは静的リンク、アイコン（`AppIcon.ico` / `TrayIcon-Run.ico` / `TrayIcon-Stop.ico`）とマニフェスト・バージョン情報は exe に埋め込まれるため、この exe だけで動作します（依存は Windows 標準の DLL のみ）。

バージョンは `Cargo.toml` の `version` が唯一の定義元で、`build.rs` が exe のバージョン情報とマニフェストに、バージョン情報画面が `CARGO_PKG_VERSION` から反映します。変更後は `Cargo.lock` の更新のため一度 `--locked` なしでビルドしてください。

Java 版・.NET 版とは `%TEMP%\W10Wheel.lock` で多重起動を排他するため、既存の W10Wheel が起動している場合は終了してから実行してください。

## テスト

```powershell
cargo test --locked
```

## 配布用 zip

```powershell
pwsh build/package.ps1 -Offline
```

`artifacts\W10Wheel.rs-<バージョン>.zip`（`W10Wheel.exe` と `Readme.ja.txt` / `License.txt` / `Library.txt` / `AccelTable.txt`）と、その SHA-256 を書いた `.zip.sha256` が生成されます。バージョンは `Cargo.toml` から読み、`dist/Readme.ja.txt` のバージョンと履歴の行が一致しない場合はエラーになります。`-Offline` は取得済みのクレートでビルドします（GitHub Actions では付けません）。PowerShell 7（`pwsh`）が必要です。

## リリース

1. `Cargo.toml` の `version` と `dist/Readme.ja.txt`（バージョンと履歴の行）を更新し、`dist/release-notes/v<バージョン>.md` にリリースノートを書きます。
2. コミットし、バージョンと同じタグを push します（例: `git tag v2.10.0` → `git push origin v2.10.0`）。
3. GitHub Actions（`.github/workflows/build.yml`）がテスト・ビルドを行い、zip と `.sha256` を添付した下書きのリリースを作成します。GitHub で内容を確認して公開してください。

push と pull request でも同じテスト・ビルドが行われ、zip と `.sha256` がワークフローの成果物としてアップロードされます。

## 実装の概要

Java 版・.NET 版 2.10.0 と同じ機能・設定ファイル・コマンドを実装しています。

- **設定ファイル（Java / .NET 版と共通）**: `%USERPROFILE%\.W10Wheel.properties`（および `.W10Wheel.<名前>.properties`）を同じ形式で読み書きします。変更されたキーの行だけを更新し、コメント・空行・`customAccelThreshold` / `customAccelMultiplier`・文字コード（UTF-8 / BOM 付き UTF-16 / UTF-32）・改行コードを保持します。保存は一時ファイルを `ReplaceFileW` で置き換えるため、書きかけのファイルは残らず、隠し属性なども保たれます。
- **設定画面・トレイ**:
  - **7 ページの設定画面**: `General` / `Trigger` / `Scroll` / `Accel Table` / `Real Wheel Mode` / `VH Adjuster` / `Excluded Programs` に加え、プロファイルの切り替え・追加・削除・フォルダを開く・再読み込み、既定値に戻す、設定キー名付きのツールチップ（無効な項目にも表示）、数値入力のデバウンス（300ms）と範囲表示。
  - **日英切り替え**: `uiLanguage`（`en` / `ja`）に応じて設定画面・確認ダイアログ・トレイメニュー・ツールチップを即時に切り替えます。日本語の文言は Java 版・.NET 版と同じです。
  - **ライト / ダークモード**: `AppsUseLightTheme` をレジストリ監視し、実行中の切り替えにも追従します。
  - **トレイ常駐**: 起動時は設定画面を開かずタスクトレイに常駐します（設定画面から管理者として再起動したときは開きます）。左ダブルクリックで一時停止の切り替え、右クリックメニュー（**設定...** / 一時停止 / バージョン情報 / 終了）。
  - **除外プログラムの選択**: ファイルダイアログ（`.exe` のみ表示、複数選択可）。ショートカット（`.lnk`）が選ばれた場合は `IShellLinkW` でリンク先の実行ファイルを追加します。
- **IPC・OS 統合**:
  - **W10Message**: `MOUSEEVENTF_HWHEEL`（`mouseData=1`、`dwExtraInfo=W10_MESSAGE_*`）で Java 版・.NET 版・Rust 版の間で `--sendExit` / `--sendPassMode [true|false]` / `--sendReloadProp` / `--sendInitState` を送受信します（一時停止中も受信可能）。
  - **管理者として再起動**: `TokenElevation` で判定し、`adminCheck=true` の起動時確認と設定画面の「管理者として再起動」（`ShellExecuteExW` + `"runas"` + `--restartAdmin`）に対応します。
  - **セッション終了（`WM_ENDSESSION`）**: フック側の隠しウィンドウ（`W10Wheel_WM`）と GUI の両方で検知し、フック解除・カーソル復元・設定保存を行います。ダイアログ表示中の終了は、ダイアログが閉じてから行います。
  - **トレイの復帰**: `ChangeWindowMessageFilter(TaskbarCreated, MSGFLT_ADD)` で、管理者として実行中も Explorer 再起動後にトレイアイコンを復帰します。
  - **フックの再登録**: スリープからの復帰（`PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND`。モダンスタンバイでも届くよう `RegisterSuspendResumeNotification` で登録）とロック解除（`WTS_SESSION_UNLOCK`）の 2 秒後と 10 秒後、また一時停止の解除時に、マウスとキーボードのフックを登録し直してフックチェーンの先頭に戻ります。X-Mouse Button Control などが復帰時にフックを登録し直すと、そちらが先に呼ばれます。割り当て直したボタンは注入された入力として届き、W10Wheel は無視するので、マウスのトリガーが効かなくなるためです。マウスもキーボードも新しいフックを登録してから古いフックを外すので、スクロール中でもそのまま続きます（登録に失敗したら古いフックを残します）。古いフックがタイムアウトなどで Windows に外されていた場合だけ、離した通知を取り逃した可能性があるので、そのフックのトリガーの状態を初期化します。
  - **ロック・スリープ中の入力**: ロック中（セキュアデスクトップ）とスリープ中の入力はフックに届かないため、ロック・ロック解除（`WTS_SESSION_LOCK` / `WTS_SESSION_UNLOCK`）とスリープ・復帰の時点で、スクロールを終え、すべてのキーとボタンの記録を初期化します（その間に離したキーやボタンの記録が残らないように）。復帰は `PBT_APMRESUMEAUTOMATIC` の後、ユーザーの操作で `PBT_APMRESUMESUSPEND` も届くことがあるため、初期化は最初の通知でだけ行います（その間に押したキーの記録が消えないように）。初期化より前に押され、すでに押されているキーは、リピートも離したときもそのまま通します。`WTSRegisterSessionNotification` は起動直後に依存サービスの準備前だと失敗することがあるため、10 秒ごとに 3 回まで再試行します。
  - **一時停止中のキーボードフック**: 一時停止中はキーボードフックを外し、解除時に登録し直します。マウスフックはコマンド（W10Message）を受け取るために残し、すべての入力をそのまま通します。
- **除外プログラム**:
  - **共通ファイル**: `%USERPROFILE%\.W10Wheel.exclusions`（および `.W10Wheel.<名前>.exclusions`）をプロファイルの読み込み・追加・削除・初期化・保存と連動して管理します（空の場合は削除、シンボリックリンクはリンクを保ったまま空にします）。
  - **判定**: `WindowFromPoint` と `GetAncestor(GA_ROOT)` でカーソル下と親のトップレベルウィンドウのプロセスの実行ファイル（`QueryFullProcessImageNameW`）を取得し、`RtlUpcaseUnicodeChar` で大文字小文字を区別せずに比較します（WebView2 など別プロセスの子ウィンドウにも対応、除外リストが空なら判定しません）。
- **トリガー・入力**:
  - **マウスのトリガー（12 種類）**: `LR` / `Left` / `Right`（`pollTimeout` の同時押し判定）、`Middle` / `X1` / `X2`（`scrollLocktime` による固定、`sendMiddleClick`）、`LeftDrag` / `RightDrag` / `MiddleDrag` / `X1Drag` / `X2Drag`（`draggedLock`、動かさずに離したらクリックを再送）、`None`。
  - **キーボードトリガー**: `WH_KEYBOARD_LL` による 23 キーから選択（押し続けた時のリピートは抑止。`VK_PAUSE` も他のキーと同じ）、マウスとキーボードのトリガーの相互キャンセル。他のソフトが送ったキーは扱いません。
  - **入力の送信**: `dwExtraInfo` にタグを付けた送信（クリックの間隔、送れなかった Up の再送、重複や単独の Up の抑止）、Escape を押しながらのクリックで状態を初期化。
- **スクロール**:
  - **通常のモードと加速テーブル**: `accelTable`, `accelMultiplier`（`M5`〜`M9`）, `customAccelTable` / `customAccelThreshold` / `customAccelMultiplier`, `verticalThreshold`, `horizontalThreshold`
  - **リアルホイールモード**: `realWheelMode`, `wheelDelta`, `vWheelMove`, `hWheelMove`, `quickFirst`, `quickTurn`
  - **方向・カーソル**: `horizontalScroll`, `reverseScroll`, `swapScroll`, `cursorChange`, `vhAdjusterMode`, `vhAdjusterMethod`（`Fixed` / `Switching`）, `firstPreferVertical`, `firstMinThreshold`, `switchingThreshold`
  - **カーソルの変更と復元**: スクロールモードの間は、システムカーソルの通常の選択・テキスト選択・リンクの選択を上下（左右）の矢印に差し替えます。終了時は `SPI_SETCURSORS` で Windows に設定されているカーソル（ユーザーのポインターの設定）を読み込み直して戻します。戻せなかった場合は、戻せるまで 150ms ごとにタイマーで再試行します（差し替えていなければ何もしません）。

## ライセンス

The MIT License（`License.txt`）。使用しているライブラリは `Library.txt` を参照してください。
