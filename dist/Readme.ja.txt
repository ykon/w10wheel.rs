名前:
        W10Wheel.rs (Rust版)

バージョン:
        2.10.0

URL:
        Blog: https://ykon0x1.blogspot.jp
        Rust版: https://github.com/ykon/w10wheel.rs
        Java版: https://github.com/ykon/w10wheel
        .NET版: https://github.com/ykon/w10wheel.net

概要:
        マウスホイールシミュレーター

履歴:
        2026-11-25: v2.10.0: 初公開 (Java版・.NET版 2.10.0 を Rust に移植)

移植:
        Java版 (Scala) と .NET版 (F#) を Rust に移植しました。
        Java版・.NET版 2.10.0 と同じ機能で、ほとんどの動作は同一となります。
        また、設定ファイルと除外リストのファイルも共有します。

対応環境:
        Windows 10 / Windows 11 (x64)
        W10Wheel.exe だけで動作します (ランタイムのインストールは不要です)。

        Windows 10 / Windows 11:
                マウスの設定で、非アクティブ ウィンドウをホバーしたときに
                スクロールするオプションを有効にしてください。
                Windows 11: 設定 > Bluetooth とデバイス > マウス
                Windows 10: 設定 > デバイス > マウス

インストール:
        Releases から W10Wheel.rs-<バージョン>.zip をダウンロードし、
        任意のフォルダに展開して W10Wheel.exe を実行してください。
         https://github.com/ykon/w10wheel.rs/releases
        W10Wheel.rs-<バージョン>.zip.sha256 に zip の SHA-256 が書かれています。
        (PowerShell の Get-FileHash で確認できます)

        SmartScreen の警告:
                W10Wheel.exe にはコード署名がないため、初めて実行したときに
                「Windows によって PC が保護されました」と表示されることがあります。
                上記の Releases からダウンロードしたものであれば、
                「詳細情報」をクリックしてから「実行」を選んでください。

        Java版・.NET版からの切り替え:
                実行中の W10Wheel を終了してください。(右クリックメニューの Exit)
                同時に起動することはできません (後から起動したほうが終了します)。
                設定ファイル (.W10Wheel.properties など) はユーザーディレクトリにあるため、
                そのまま引き継がれます。
                スタートアップなどのショートカットから起動している場合は、
                リンク先を新しい W10Wheel.exe に変更してください。

互換性:
        Logitech(ロジクール) の SetPoint は問題ありません。
        # 環境によっては駄目な場合もあるようです。
        「ボタンの切り替え」などをしてもうまく動きます。
        フィルタドライバのレベルで動いているものは、恐らく安全です。

        グローバルフックのレベルで動いているものは競合します。
        ただし、WizMouse などは安全に使えます。

        WheelBall や Nadesath など同種のソフトは、同時に起動しないでください。
        動作が競合して操作不能になります。

回復操作:
        スクロールモードのまま戻らなくなったら、
        Escape キーを押しながらクリックを試してください。
        状態を初期化して、スクロールモードを終了します。

        操作が効かなくなったら Ctrl-Alt-Delete でタスクマネージャーを開いてください。
        大抵の場合は、それだけで制御を取り戻せます。
        効かない場合は、何回か繰り返してください。
        それでも駄目な場合は、タスクマネージャーでアイコンを探し、
        プロセスを終了させてください。
        # "W10Wheel.exe --sendExit" で終了することも可能です。

        WheelBall などと同時に起動すると、何故か終了できなくなることがあります。
        この場合の終了方法は OS の再起動しかありません。
        # 他に何か方法があったら教えてください。

使用方法:
        W10Wheel.exe を実行してください。
        管理者として実行していない場合は、起動時に確認が表示されます。
        (管理者として実行されているプログラムのウィンドウでは、管理者として実行しないと機能しません)
        「管理者として再起動」(Restart as administrator) を選ぶと、
        UAC の確認のあと、管理者として起動し直します。
        「次回から表示しない」(Do not ask again) にチェックして「このまま実行」を選ぶと、
        以後は確認しません。
        (設定ダイアログの「全般」ページで元に戻せます)
        なお、別のアカウント (管理者) のパスワードを入力して管理者として実行した場合は、
        そのアカウントのユーザーディレクトリの設定ファイルが使われます。
        タスクトレイにアイコンが表示されます。
        右クリックメニューの Settings... で設定ダイアログを開き、設定を変更できます。
        詳しい方は、設定ファイル (.W10Wheel.properties) を直接編集してください。
        設定ダイアログで設定を変更するとすぐに反映され、保存されます。
        設定を変更するか、一度起動して終了すれば、設定ファイルはユーザーディレクトリに生成されます。
        実行中に設定ファイルを編集した場合は、設定ダイアログの Reload か
        --sendReloadProp コマンドで読み込み直してください。
        設定ファイルには W10Wheel で変更した設定だけが書き込まれ、
        ファイルで編集した他の設定やコメントはそのまま残ります。
        (同じ設定を両方で変更した場合は、W10Wheel で変更した値になります)
        設定ダイアログとメニューは Windows のアプリのモード (ライト / ダーク) に合わせて表示されます。

        既定のマウスのトリガーは RightDrag です。右ボタンを押したままマウスまたはボールを動かすと
        スクロールし、動かさずに離すと右クリックになります。
        トリガーは設定ダイアログの「トリガー」ページで変更できます。

        あとの使い方は WheelBall など同種のソフトと同様になります。
        各トリガーを押すと、スクロールモードに移行します。
        マウスまたはボールの操作でスクロールします。
        スクロールモードは何かのボタンを押すと解除されます。

        トリガーを押したままスクロールして、離したら止めることもできます。
        同時押しでは、両方を押さえたままにするのではなく、片方を先に離すと使いやすくなります。

        トリガーを押したままスクロールしている間に、別のトリガー (マウスのトリガーと
        キーボードトリガー) を押すと、スクロールモードを終了します。

        *Dragのトリガーではドラッグしている間だけスクロールします。
        スクロールモードに固定されません。(Dragged Lock が無効の場合)

        タスクトレイのアイコンをダブルクリックするか、右クリックメニューの Pause を選択すると
        PassMode (一時停止) を切り替えます。
        PassModeを有効にするとアイコンが赤色に変更されます。

        終了するには、右クリックメニューから Exit を選択してください。

コマンド:
        "--"が先頭に付いた文字列を実行ファイルの引数に渡すとコマンドと解釈します。

        --send*のコマンドは実行したら、そのまま終了します。
        Java版、.NET版、Rust版で相互に機能します。
        送信に失敗した場合は、終了コード 1 を返します。

        --sendPassMode [true | false]
        # --sendPassMode true // PassMode有効
        実行中のプロセスにPassModeを設定します。
        第二引数を省略した場合はtrueと見なします。

        --sendReloadProp
        実行中のプロセスに、設定ファイルの再読込を指示します。

        --sendInitState
        実行中のプロセスに、状態の初期化を指示します。

        --sendExit
        実行中のプロセスを終了します。

設定ファイル:
        設定ダイアログの Properties で他のPropertiesを選択すると
        現在の設定は保存されて
        選択されたPropertiesから設定が読み込まれます。

        Defaultは".W10Wheel.properties"になります。
        # これは使えません => .W10Wheel.Default.properties

        Addで追加される、Propertiesファイルの名前の書式です。
        # 正規表現: \.W10Wheel\.(?!--)(.+)\.properties
        Left = .W10Wheel.Left.properties
        Right = .W10Wheel.Right.properties
        Test = .W10Wheel.Test.properties
        ファイル名の2回目の"."と3回目の"."の間が定義された(表示される)名前となります。
        コマンドを区別するため、先頭に"--"は使えません。
        # これは使えません => .W10Wheel.--Test.properties
        ファイル名に使えない文字 (\ / : * ? " < > |) や前後の空白も使えません。
        Default は大文字小文字に関係なく使えません。

        "--"が先頭に付かない文字列を実行ファイルの引数に渡すと、Propertiesを選択できます。
        引数を渡さない場合は、終了時の設定に関わらず、Defaultで起動します。

        設定ファイルを読み込めない場合 (他のソフトが開いている、削除されたなど) は、
        エラーを表示して、設定も選択中の Properties も変更しません。
        起動時に読み込めなかった場合は既定値で起動し、終了時に設定を保存しません。
        設定ファイルにない項目や正しくない値には既定値が使われます。

        設定ファイルは Java版・.NET版と同じ書式で読み書きします (UTF-8。BOM 付きの UTF-16 / UTF-32 も
        読めて、その場合はその文字コードのまま書き込みます)。
        1行に "項目=値" を書き、"#" から行末まではコメントです (値の後ろにも書けます)。

        除外リスト (「除外するプログラム」ページ) は、Propertiesごとに別のファイルに保存されます。
        (Java版・.NET版と同じファイルです)
        Default = .W10Wheel.exclusions
        Test = .W10Wheel.Test.exclusions
        1行に1つ、.exe のフルパスを書きます (大文字と小文字は区別しません)。
        前後の空白と空行は無視されます。コメントは書けません。
        一覧を空にするとファイルは削除されます (ファイルがない場合は登録なし)。
        Add で Properties を追加すると除外リストもコピーされ、Delete で一緒に削除されます。
        ファイルを読み込めない場合 (他のソフトがロックしている場合など) は、除外リストは空になり
        (Reload のときは今の除外リストのまま)、変更は保存されません (Reload で読み込み直せます)。

メニュー項目:
        Settings...: 設定ダイアログを開く
        Pause: 全てのメッセージをそのまま通す (PassMode) # WheelBall の制御停止
        About: バージョン番号、管理者として実行しているかを表示
        Exit: 終了

設定ダイアログ:
        設定を変更するとすぐに反映され、設定ファイルに保存されます。
        左の一覧でページを選びます。
        ダーク/ライトの表示は Windows の設定に従います。
        (実行中に Windows で切り替えた場合も反映されます)
        Reset to defaults: 選んでいる設定ファイルの設定を既定値に戻す (言語は除く)
        Properties: 設定ファイルの切り替え、Add (追加)、Delete (削除)、
                Open Dir (フォルダを開く)、Reload (再読み込み)
        「全般」ページ:
                Language: 言語設定
                Priority: プロセスの優先度を変更
                Restart as administrator: 管理者として起動し直す (管理者として実行されていないとき)
                Ask to restart as administrator at startup: 起動時の管理者の確認 (設定項目を参照)
        「トリガー」ページ:
                Mouse: マウスのトリガーを変更 (設定項目を参照)
                        チェックを外すとマウスのトリガーなし (NoneTrigger)
                Keyboard: キーボードトリガー
                pollTimeout, scrollLocktime: 数値をセット (設定項目を参照)
        「スクロール」ページ:
                Cursor Change: スクロールモードのカーソル変更
                Horizontal Scroll: 水平スクロール
                Reverse Scroll (Flip): スクロールの方向を逆にする (反転する)
                Swap Scroll (V.H): 垂直スクロールと水平スクロールを入れ替える (Vertical <-> Horizontal)
                verticalThreshold, horizontalThreshold: 数値をセット (設定項目を参照)
        「加速テーブル」ページ: 加速を有効にするか、どのテーブルを使うか # 1.0は1.0倍を表す
        「リアルホイールモード」ページ: 実際のホイールに近いスクロール (設定項目を参照)
        「垂直/水平スクロール調整」ページ: 垂直、水平の方向固定、切り替え機能
        「除外するプログラム」ページ: スクロール機能を無効にするプログラム (設定項目の除外リストを参照)
                Add...: プログラム (.exe) を選んで追加
                Remove: 選んだプログラムを一覧から削除

設定項目:
        firstTrigger: string (default: RightDragTrigger)
                LRTrigger: # 同時押し
                        左から右か、右から左を押すとトリガーになります。
                        左、右クリックともに次のイベントを待つために遅延します。
                LeftTrigger: # 同時押し
                        左から右を押すとトリガーになります。
                        右からはトリガーになりません、そのため右クリックの遅延を解消できます。
                RightTrigger: # 同時押し
                        右から左を押すとトリガーになります。
                        左からはトリガーになりません、そのため左クリックの遅延を解消できます。
                MiddleTrigger:
                        中ボタンを押すとトリガーになります。
                X1Trigger:
                        X1を押すとトリガーになります。
                X2Trigger:
                        X2を押すとトリガーになります。
                LeftDragTrigger: # 固定なし
                        左ボタンでドラッグするとスクロールできます。
                        固定はされません、ドラッグしないで離すと左クリックを送ります。
                RightDragTrigger: # 固定なし
                        右ボタンでドラッグするとスクロールできます。
                        固定はされません、ドラッグしないで離すと右クリックを送ります。
                MiddleDragTrigger: # 固定なし
                        中ボタンでドラッグするとスクロールできます。
                        固定はされません、ドラッグしないで離すと中クリックを送ります。
                X1DragTrigger: # 固定なし
                        X1ボタンでドラッグするとスクロールできます。
                        固定はされません、ドラッグしないで離すとX1クリックを送ります。
                X2DragTrigger: # 固定なし
                        X2ボタンでドラッグするとスクロールできます。
                        固定はされません、ドラッグしないで離すとX2クリックを送ります。
                NoneTrigger:
                        マウスのトリガーなし (キーボードトリガーだけで使う場合)

        processPriority: string (default: AboveNormal)
                High: 高
                AboveNormal: 通常以上
                Normal: 通常

        pollTimeout: 50-500 (default: 200)
                同時押しのイベント待ち時間(ミリ秒)  # WheelBall の 判定時間
        scrollLocktime: 150-500 (default: 200)
                トリガーを離してスクロールモードに固定する時間(ミリ秒)
                この時間以内にトリガーを離すとスクロールモードに固定します。
        realWheelMode: bool (default: false)
                実際のホイールに近いスクロール
                こちらのモードではAccel値は使われません。
        cursorChange: bool (default: true)
                スクロールモードのカーソル変更
        verticalThreshold: 0-500 (default: 0)
                垂直(通常)スクロールの閾値
        horizontalScroll: bool (default: true)
                水平スクロール
                使わない人は無効にしてください。
        horizontalThreshold: 0-500 (default: 75)
                水平スクロールの閾値
                この値をあまり小さくすると垂直(通常)スクロールが、使いづらくなります。
        reverseScroll: bool (default: false)
                スクロールの方向を逆にする (反転する)
        swapScroll: bool (default: false)
                垂直スクロールと水平スクロールを入れ替える (Vertical <-> Horizontal)

        sendMiddleClick: bool (default: false) # Middle, X1, X2
                Shift または Ctrl または Alt キーを押しながら
                トリガーを押すと中クリックを送ります。

        draggedLock: bool (default: false) # *DragTrigger
                *DragTriggerでドラッグ後、スクロールモードに固定します。

        vhAdjusterMethod: string (default: Switching) # VHAdjuster
                Fixed: 初回の移動量が多い方向に固定
                Switching: 方向固定 + 切り替え (switchingThreshold参照)

        vhAdjusterMode: bool (default: false) # VHAdjuster
                VHAdjusterの有効、無効
        firstPreferVertical: bool (default: true) # VHAdjuster
                初回判定時に垂直(縦)を優先する
        firstMinThreshold: 1-10 (default: 5) # VHAdjuster
                初回を判定する移動量 (この値以下は無視する)
        switchingThreshold: 10-500 (default: 50) # VHAdjuster
                この値を超える入力で、方向を切り替える

        wheelDelta: 10-500 (default: 120) # RealWheelMode
                RealWheelModeでの一回分のホイール値
                通常のマウスのホイール値は120です。
        vWheelMove: 10-500 (default: 60) # RealWheelMode
                垂直(通常)スクロール、一回分のホイールに変換する移動量
        hWheelMove: 10-500 (default: 60) # RealWheelMode
                水平スクロール、一回分のホイールに変換する移動量
        quickFirst: bool (default: false) # RealWheelMode
                初回の反応を速くするか
                移動量に関係なくホイールを送ります。
                falseでも初回は半分の移動量に設定されます。
        quickTurn: bool (default: false)  # RealWheelMode
                折り返しの反応を速くするか
                移動量に関係なくホイールを送ります。

        accelTable: bool (default: true) # AccelTable
                AccelTableを有効にするか
        customAccelTable: bool (default: false) # AccelTable
                CustomTableを有効にするか
        accelMultiplier: string (default: M5) # AccelTable
                選択されている乗数テーブル (AccelTable.txt を参照)
        customAccelThreshold: Int Array # AccelTable
                CustomTableで使われるThreshold
                0 以上の数を小さい順に (同じ数を繰り返さずに) 指定します。
        customAccelMultiplier: Double Array # AccelTable
                CustomTableで使われるMultiplier
                0 以上の数を Threshold と同じ個数だけ指定します。
                正しくない値がある場合、CustomTable は使われません。

        keyboardHook: bool (default: false) # Keyboard
                キーボードトリガーを有効にする
                他のソフトが送ったキー (パスワードマネージャーの自動入力やマクロなど) は
                トリガーとして扱いません。キーを送る方式のツール (PowerToys の
                Keyboard Manager や AutoHotkey など) で割り当てたキーもトリガーになりません。
        targetVKCode: string (default: VK_NONCONVERT) # Keyboard
                トリガーに使うキー

        uiLanguage: string (default: by system) # Language
                UIの表示に使う言語設定

        除外リスト (.W10Wheel.exclusions など、「設定ファイル」を参照) # Excluded Programs
                登録したプログラムのウィンドウ上ではスクロール機能が無効になる
                (すべてのトリガーで、ボタンやキーは本来の動作を行う)
                トリガーを押したときのカーソルの下のウィンドウで判定します。
                UWP / ストアアプリ (電卓、設定など) は非対応です。
                ジャンクション、シンボリックリンク、subst のドライブを経由したパス
                (Scoop の current フォルダなど) では一致しません。実際のパスを登録してください。
                以前の excludeAvast は使われません。Avast を除外する場合は AvastUI.exe を登録してください。

        adminCheck: bool (default: true) # General
                管理者として実行されていないとき、起動時に管理者として再起動するか確認する

ライセンス:
        The MIT License
        詳しくは License.txt を参照

ライブラリ:
        Library.txt を参照

アイコン:
        .NET版と同じアイコンです (.NET版のソースの build/IconGen で描いています)。

.exe:
        Rust (MSVC) でビルドした単一の exe です。
        C ランタイムを静的リンクし、アイコンなども exe に含めているため、
        W10Wheel.exe だけで動作します。

連絡:
        何かの問題や、気になる点、また改良案などがあったら
        Blog、GitHub、メールなどで報告頂けると幸いです。

製作者:
        Yuki Ono <ykon0x1@gmail.com>

著作権:
        Copyright (c) 2016 Yuki Ono
