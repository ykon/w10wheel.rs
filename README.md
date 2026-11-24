# W10Wheel Rust 版 (Phase 6)

Windows 10 / 11 x64 向けの W10Wheel Rust 実装です。Rust の MSVC ツールチェーンと Windows SDK でビルドできます。

## ビルド

```powershell
cargo build --release --offline --locked
```

単一の GUI バイナリ `.\target\release\W10Wheel.exe` が生成されます。従来の W10Wheel（Java 版 / .NET 版）とは `%TEMP%\W10Wheel.lock` で多重起動を排他するため、既存の W10Wheel が起動している場合は終了してから実行してください。

## 実行と設定ファイル（Java / .NET 版と完全共通）

```powershell
.\target\release\W10Wheel.exe
```

- **設定ファイル**: `%USERPROFILE%\.W10Wheel.properties`（およびプロファイル指定時の `.W10Wheel.<名前>.properties`）を Java / .NET 版と完全共通形式で読み書きします。変更されたキーの行だけを更新し、コメント・空行・`customAccelThreshold` / `customAccelMultiplier`・文字コード（UTF-8 / UTF-16 / UTF-32 BOM）・改行コードを保持します。
- **7 ページ本格 GUI・多言語・ダークモード・トレイ統合（Phase 6 実装済み）**:
  - **7 ページ設定画面**: `General` / `Trigger` / `Scroll` / `Accel Table` / `Real Wheel Mode` / `VH Adjuster` / `Excluded Programs` の全 7 ページに加え、プロファイルバー（切替・追加・削除・フォルダを開く・再読込）、既定値に戻す、`(設定キー名)` 付きツールチップ、数値入力デバウンス（300ms）＋範囲ラベルを備えます。
  - **日英切り替え（UI 言語）**: `uiLanguage` (`en` / `ja`) に応じて設定画面・確認ダイアログ・トレイメニュー・ツールチップをリアルタイムに切り替えます。
  - **ライト / ダークモード動的追従**: `AppsUseLightTheme` レジストリ監視によりテーマ変更を即時検知し、ComCtl32 v6 マニフェスト＋タイトルバー／コントロール配色／カスタムチェックボックス・ラジオ描画／標準コンテキストメニューへ反映します。
  - **高 DPI アイコン＆トレイ常駐**: .NET 版と共通の `AppIcon.ico` / `TrayIcon-Run.ico` / `TrayIcon-Stop.ico` を埋め込み、起動時は設定画面を開かずタスクトレイへ常駐し（`--openSettings` 指定時除く）、トレイ左ダブルクリックで一時停止／再開トグル、右クリックメニュー（太字 **設定...** / チェック付き **一時停止** / セパレータ / **バージョン情報** / **終了**）を提供します。
  - **除外プログラム選択**: 複数選択ファイルダイアログ＋COM `IShellLinkW` により `.exe` および `.lnk` ショートカットのリンク先実行ファイルを追加できます。
- **IPC・OS 統合（Phase 5 実装済み）**:
  - **W10Message IPC 相互運用**: `MOUSEEVENTF_HWHEEL` (`mouseData=1`, `dwExtraInfo=W10_MESSAGE_*`) により Java 版・.NET 版・Rust 版間で `--sendExit` / `--sendPassMode [true|false]` / `--sendReloadProp` / `--sendInitState` を相互送受信できます（一時停止中も受信可能）。
  - **管理者権限チェック & UAC 再起動**: トークン昇格状態 (`TokenElevation`) を判定し、`adminCheck=true` の起動時確認ダイアログおよび設定画面からの「管理者として再起動」（`ShellExecuteExW` + `"runas"` + `--restartAdmin --openSettings`）に対応します。
  - **セッション終了 (`WM_ENDSESSION`)**: フック側トップレベル隠しウィンドウ (`W10Wheel_WM`) と GUI ウィンドウの両方でログオフ／シャットダウンを検知し、フック解除・カーソル復元・設定保存を確実に実行します。
  - **UIPI トレイ復帰**: `ChangeWindowMessageFilter(TaskbarCreated, MSGFLT_ADD)` により、管理者昇格時も Explorer 再起動後のトレイアイコン復帰を受信します。
- **除外プログラム（Phase 4 実装済み）**:
  - **共通ファイル**: `%USERPROFILE%\.W10Wheel.exclusions`（および `.W10Wheel.<名前>.exclusions`）をプロファイルの読込・再読込・追加・削除・初期化・保存と連動して管理します（空時は通常ファイルを削除、シンボリックリンク時はリンクを保ったまま空書き）。
  - **判定・照合**: `WindowFromPoint` と `GetAncestor(GA_ROOT)` でカーソル直下および親トップレベルウィンドウのプロセス実行ファイル（`QueryFullProcessImageNameW`）を取得し、`RtlUpcaseUnicodeChar` による NTFS 準拠の大文字小文字非依存比較を行います（WebView2 等の別プロセス子ウィンドウ対応、除外リスト空時は API 呼出ゼロで即スキップ）。
- **トリガー・入力エンジン（Phase 3 実装済み）**:
  - **全 12 マウストリガー**: `LR` / `Left` / `Right`（`pollTimeout` 同時押し判定）, `Middle` / `X1` / `X2`（`scrollLocktime` Released Scroll Mode / `sendMiddleClick` 修飾クリック）, `LeftDrag` / `RightDrag` / `MiddleDrag` / `X1Drag` / `X2Drag`（`draggedLock` / 未移動クリック再送）, `None`
  - **キーボードトリガー**: `WH_KEYBOARD_LL` による 23 キー選択、`VK_PAUSE` 反復制御、マウス ⇔ キーボード相互キャンセル
  - **信頼性・状態復帰**: `dwExtraInfo` タグ付き `InputSender`（クリック遅延・未送信 Up リトライ・`ButtonTracker` 重複/孤立 Up 抑止）、`Escape + クリック` による状態リセット
- **スクロール計算エンジン（Phase 2 実装済み）**:
  - **Direct モード + 加速テーブル**: `accelTable`, `accelMultiplier` (`M5`〜`M9`), `customAccelTable` / `customAccelThreshold` / `customAccelMultiplier`, `verticalThreshold`, `horizontalThreshold`
  - **RealWheel モード**: `realWheelMode`, `wheelDelta`, `vWheelMove`, `hWheelMove`, `quickFirst`, `quickTurn`
  - **方向・カーソル制御**: `horizontalScroll`, `reverseScroll`, `swapScroll`, `cursorChange`, `vhAdjusterMode`, `vhAdjusterMethod` (`Fixed` / `Switching`), `firstPreferVertical`, `firstMinThreshold`, `switchingThreshold`
  - **カーソル復元**: `SPI_SETCURSORS` 方式＋リトライタイマーで標準カーソルへ安全に復元します。

## テスト

```powershell
cargo test --offline
```
