use crate::config::{CustomTableError, CustomTableState, UiLanguage};
use crate::properties::{ProfileNameError, StoreStatus};

pub fn tr(lang: UiLanguage, key: &'static str) -> &'static str {
    if lang == UiLanguage::En {
        return key;
    }
    match key {
        // General / Tray / App
        "Runnable" => "実行中",
        "Stopped" => "停止中",
        "Settings..." => "設定...",
        "Settings" => "設定",
        "Pause" => "一時停止",
        "About" => "バージョン情報",
        "Exit" => "終了",
        "OK" => "OK",
        "Cancel" => "キャンセル",
        "Yes" => "はい",
        "No" => "いいえ",
        "Error" => "エラー",
        "Command Error" => "コマンドのエラー",
        "Default" => "既定",
        "Switch" => "切り替える",
        "Delete" => "削除",
        "Reset" => "リセット",

        // Startup & Dialog Errors
        "W10Wheel is already running. This instance will exit." => "二重起動を検出したため終了します。",
        "Failed to create system tray icon" => "タスクトレイのアイコンを作成できませんでした",
        "The settings folder is not available" => "設定フォルダーを利用できません",
        "Default properties cannot be deleted" => "既定の設定ファイルは削除できません",
        "W10Wheel is not running as administrator" => "W10Wheel は管理者として実行されていません",
        "So it won't work in the windows of some programs (those running as administrator). It's highly recommended to run it as administrator." => {
            "そのため、一部のプログラム (管理者として実行されているもの) のウィンドウでは機能しません。管理者として実行することを推奨します。"
        }
        "More info (Wiki)" => "詳細情報 (Wiki)",
        "Restart as administrator" => "管理者として再起動",
        "Windows asks for permission (User Account Control)" => "UAC の確認が表示されます",
        "Continue without administrator rights" => "このまま実行",
        "Do not ask again" => "次回から表示しない",
        "Ask to restart as administrator at startup" => "起動時に管理者として再起動するか確認する",
        "Asked when W10Wheel is not running as administrator" => {
            "W10Wheel が管理者として実行されていない場合に確認します"
        }
        "Restarting loses the changes that could not be saved." => {
            "再起動すると、保存できなかった変更は失われます。"
        }
        "Running as administrator" => "管理者として実行",
        "Version" => "バージョン",
        "Architecture" => "アーキテクチャ",
        "Failed mouse hook install" => "マウスフックのインストールに失敗しました",
        "Failed keyboard hook install" => "キーボードフックのインストールに失敗しました",
        "Failed to restart as administrator" => "管理者として再起動できませんでした",
        "Failed to send IPC command." => "IPC コマンドの送信に失敗しました。",
        "Properties does not exist" => "設定ファイルが存在しません",
        "Unknown Command" => "不明なコマンド",
        "Invalid Command Argument" => "コマンド引数が不正です",

        // Profile Bar & Bottom Bar
        "Properties" => "プロファイル",
        "Add" => "追加",
        "Open Dir" => "フォルダを開く",
        "Reload" => "再読込",
        "Reset to defaults" => "既定値に戻す",
        "Close" => "閉じる",
        "Changes are applied and saved at once" => "変更はすぐに反映・保存されます",
        "The properties file could not be read: changes are not saved" => {
            "設定ファイルを読み込めませんでした (変更は保存されません)"
        }
        "The changes could not be saved to the properties file" => {
            "設定ファイルに保存できませんでした"
        }
        "The changes could not be saved" => "変更を保存できませんでした",
        "The exclusions file could not be read: changes are not saved" => {
            "除外設定ファイルを読み込めませんでした (除外の変更は保存されません)"
        }
        "The changes could not be saved to the exclusions file" => {
            "除外設定ファイルに保存できませんでした"
        }
        "The properties file could not be read" => "設定ファイルを読み込めませんでした",
        "The exclusions file could not be read" => "除外設定ファイルを読み込めませんでした",
        "Switching loses the changes that could not be saved." => {
            "切り替えると、保存できなかった変更は失われます。"
        }
        "Properties Name" => "設定ファイル名",
        "Add Properties" => "設定ファイル追加",
        "Delete Properties" => "設定ファイル削除",
        "Delete these properties?" => "この設定ファイルを削除しますか？",
        "Reset all settings and excluded programs to defaults?" => {
            "すべての設定と除外プログラムを既定値に戻しますか？"
        }
        "Contains characters not allowed in file names" => {
            "ファイル名に使えない文字が含まれています"
        }
        "Spaces at either end cannot be used" => "前後に空白は使えません",
        "Cannot start with --" => "先頭に -- は使えません",
        "Default cannot be used" => "Default は使えません",
        "Properties of this name already exist" => "この名前の設定ファイルは既に存在します",

        // 7 Pages
        "General" => "全般",
        "Trigger" => "トリガー",
        "Scroll" => "スクロール",
        "Accel Table" => "加速テーブル",
        "Real Wheel Mode" => "リアルホイールモード",
        "VH Adjuster" => "垂直/水平スクロール調整",
        "Excluded Programs" => "除外するプログラム",

        // General Page
        "Language" => "言語",
        "English" => "English",
        // The mouse triggers with how their buttons are pressed (as the .NET version)
        "LR (Left <<-->> Right)" => "LR (左 <<-->> 右)",
        "Left (Left -->> Right)" => "Left (左 -->> 右)",
        "Right (Right -->> Left)" => "Right (右 -->> 左)",
        "X1" => "X1 (拡張1)",
        "X2" => "X2 (拡張2)",
        "Japanese" => "日本語",
        "Priority" => "優先度",
        "High" => "高 (High)",
        "Above Normal" => "通常以上 (Above Normal)",
        "Normal" => "通常 (Normal)",
        "The language of the menu and the dialogs" => "メニューとダイアログの言語です",
        "The priority of the process of W10Wheel" => "W10Wheel のプロセスの優先度です",

        // Trigger Page
        "Mouse" => "マウス",
        "Keyboard" => "キーボード",
        "Send MiddleClick" => "中クリックを送信",
        "Dragged Lock" => "ドラッグ後にロック",
        "Simultaneous press time" => "同時押し判定時間",
        "Scroll mode lock time" => "スクロール固定時間",
        "Scrolling is disabled" => "スクロール機能が無効になっています",
        "The mouse buttons that start scroll mode\nUnchecked: no mouse trigger (the keyboard trigger only)" => {
            "スクロールモードを始めるマウスのボタンです\nチェックを外すとマウスのトリガーを使いません (キーボードトリガーのみの場合)"
        }
        "The trigger pressed with Shift, Ctrl or Alt sends a middle click (Middle, X1, X2 only)" => {
            "Shift、Ctrl、Alt キーを押しながらトリガーを押すと中クリックを送ります (Middle、X1、X2 のみ)"
        }
        "After a drag with a Drag trigger, scroll mode stays locked" => {
            "Drag のトリガーでドラッグした後、スクロールモードに固定します"
        }
        "The key that starts scroll mode\nThe keys sent by other software (autofill, macros, key remapping tools) are not triggers" => {
            "スクロールモードを始めるキーです\n他のソフトが送ったキー (自動入力、マクロ、キー割り当てツールなど) はトリガーになりません"
        }
        "How long the LR, Left and Right triggers wait for the other button" => {
            "LR、Left、Right のトリガーで、同時押しのもう一方のボタンを待つ時間です"
        }
        "A trigger released within this time locks scroll mode" => {
            "この時間内にトリガーを離すと、スクロールモードに固定します"
        }

        // Scroll Page
        "Cursor Change" => "カーソル変更",
        "Horizontal Scroll" => "水平スクロール",
        "Reverse Scroll (Flip)" => "スクロール反転 (Flip)",
        "Swap Scroll (V.H)" => "垂直・水平入替 (V.H)",
        "Vertical scroll threshold" => "垂直スクロール閾値",
        "Horizontal scroll threshold" => "水平スクロール閾値",
        "The thresholds are not used while VH Adjuster is enabled" => {
            "垂直/水平スクロール調整が有効な場合、閾値は使われません"
        }
        "Changes the cursor in scroll mode" => "スクロールモードのカーソルを変更します",
        "Scrolls horizontally too\nDisable it if you do not use it" => {
            "水平にもスクロールします\n使わない場合は無効にしてください"
        }
        "Reverses the direction of the scroll" => "スクロールの方向を逆にします",
        "Swaps the vertical and the horizontal scroll" => "垂直スクロールと水平スクロールを入れ替えます",
        "A vertical move up to this does not scroll" => "この値以下の垂直の移動ではスクロールしません",
        "A horizontal move up to this does not scroll\nToo small, the vertical scroll becomes hard to use" => {
            "この値以下の水平の移動ではスクロールしません\n小さすぎると垂直スクロールが使いづらくなります"
        }

        // Accel Table Page
        "Enabled" => "有効",
        "Custom Table" => "カスタムテーブル",
        "Not used while Real Wheel Mode is enabled" => {
            "リアルホイールモードが有効な場合は使われません"
        }
        "Accelerates the scroll by the speed of the move, with the multipliers of the table" => {
            "移動の速さに応じて、テーブルの倍率でスクロールを加速します"
        }
        "The multipliers of the table" => "テーブルの倍率です",
        "Uses customAccelThreshold and customAccelMultiplier of the properties file (only when they are valid)" => {
            "設定ファイルの customAccelThreshold と customAccelMultiplier を使います (正しい値が存在する場合だけ選択可能)"
        }
        "Not in the properties file" => "設定ファイルに存在しません",
        "Not used: the numbers of thresholds and multipliers differ" => {
            "使われていません: 閾値と倍率の個数が違います"
        }
        "Not used: the thresholds must be ascending numbers of 0 or more" => {
            "使われていません: 閾値は 0 以上の昇順にしてください"
        }
        "Not used: the multipliers must be numbers of 0 or more" => {
            "使われていません: 倍率は 0 以上の数にしてください"
        }
        "Not used: customAccelThreshold and customAccelMultiplier are both needed" => {
            "使われていません: customAccelThreshold と customAccelMultiplier の両方が必要です"
        }
        "Not used: a value is not a number" => "使われていません: 数値でない値があります",

        // Real Wheel Mode Page
        "Wheel delta" => "ホイール移動量",
        "Vertical move per notch" => "1 ノッチあたりの垂直移動量",
        "Horizontal move per notch" => "1 ノッチあたりの水平移動量",
        "Quick first response" => "初回即応 (Quick first)",
        "Quick turn response" => "折り返し即応 (Quick turn)",
        "Scrolls as a real wheel does, a notch at a time\nThe accel table is not used" => {
            "実際のホイールのように一回分ずつスクロールします\n加速テーブルは使われません"
        }
        "The wheel value of a notch (a usual mouse sends 120)" => {
            "一回分のホイール値です (通常のマウスは 120)"
        }
        "The vertical move made into a notch" => "一回分の垂直スクロールに変換する移動量です",
        "The horizontal move made into a notch" => "一回分の水平スクロールに変換する移動量です",
        "Sends the first notch whatever the move (else after half of the move)" => {
            "移動量が小さくても最初の 1 ノッチを即座に送ります (無効時は半分の移動後)"
        }
        "Sends a notch at once when the move turns back, whatever the move" => {
            "逆方向へ切り返したとき、即座に 1 ノッチを送ります"
        }

        // VH Adjuster Page
        "Fixed" => "固定 (Fixed)",
        "Switching" => "切替 (Switching)",
        "Prefer vertical first" => "初回は垂直を優先",
        "First direction threshold" => "初回方向判定の閾値",
        "Switching threshold" => "方向切替の閾値",
        "Used when Horizontal Scroll is enabled" => "水平スクロールが有効な場合に使われます",
        "Scrolls in one direction at a time, vertical or horizontal" => {
            "垂直または水平の一方向ずつスクロールします"
        }
        "Fixed: the direction of the larger first move\nSwitching: also switched by a move above the switching threshold" => {
            "Fixed: 最初に大きく動かした方向に固定\nSwitching: 切替閾値を超えた移動で方向を切り替え"
        }
        "The first direction prefers vertical" => "最初の方向判定で垂直を優先します",
        "The first direction is decided by a move above this (smaller moves are ignored)" => {
            "この値を超える移動で最初の方向を決めます (小さい動きは無視)"
        }
        "A move above this switches the direction (Switching)" => {
            "この値を超える移動で方向を切り替えます (Switching)"
        }

        // Excluded Programs Page
        "Add..." => "追加...",
        "Remove" => "削除",
        "Select programs to exclude" => "除外するプログラムを選択",
        "Programs (*.exe)" => "プログラム (*.exe)",
        "All files (*.*)" => "すべてのファイル (*.*)",
        "These files are not programs (.exe): they were not added" => {
            "次のファイルはプログラム (.exe) ではないため追加されませんでした"
        }
        "Scrolling is disabled in these programs" => "これらのプログラムではスクロールが無効になります",
        "The programs (.exe) whose windows have scrolling disabled\nJudged by the window under the cursor when a trigger is pressed\n(UWP / Store apps are not supported)" => {
            "スクロールを無効にするプログラム (.exe) です\nトリガー押下時のカーソル下のウィンドウで判定されます\n(UWP / ストアアプリは対象外です)"
        }

        _ => key,
    }
}

pub fn store_status_key(status: StoreStatus) -> &'static str {
    match status {
        StoreStatus::PropertiesUnreadableNotSaved => {
            "The properties file could not be read: changes are not saved"
        }
        StoreStatus::PropertiesSaveFailed => "The changes could not be saved to the properties file",
        StoreStatus::ExclusionsUnreadableNotSaved => {
            "The exclusions file could not be read: changes are not saved"
        }
        StoreStatus::ExclusionsSaveFailed => "The changes could not be saved to the exclusions file",
        StoreStatus::PropertiesReadFailed => "The properties file could not be read",
    }
}

pub fn profile_name_error_key(err: ProfileNameError) -> &'static str {
    match err {
        ProfileNameError::LeadingOrTrailingSpaces => "Spaces at either end cannot be used",
        ProfileNameError::StartsWithDoubleDash => "Cannot start with --",
        ProfileNameError::ReservedDefault => "Default cannot be used",
        ProfileNameError::InvalidChars => "Contains characters not allowed in file names",
        ProfileNameError::AlreadyExists => "Properties of this name already exist",
    }
}

pub fn format_tooltip(lang: UiLanguage, tip_key: &'static str, prop_key: Option<&str>) -> String {
    let body = tr(lang, tip_key).replace('\n', "\r\n");
    match prop_key {
        Some(key) => format!("{body}\r\n({key})"),
        None => body,
    }
}

pub fn format_tray_tooltip(lang: UiLanguage, paused: bool, profile: &str) -> String {
    let state = tr(lang, if paused { "Stopped" } else { "Runnable" });
    let mut full = if profile == "Default" {
        format!("W10Wheel - {state}")
    } else {
        format!("W10Wheel - {state} ({profile})")
    };
    if full.encode_utf16().count() > 127 {
        let mut units = 0usize;
        let mut cut = 0usize;
        for (idx, ch) in full.char_indices() {
            let len = ch.len_utf16();
            if units + len > 124 {
                break;
            }
            units += len;
            cut = idx + ch.len_utf8();
        }
        full.truncate(cut);
        full.push_str("...");
    }
    full
}

fn format_multiplier(val: f64) -> String {
    let rounded = (val * 1_000_000.0).round() / 1_000_000.0;
    if (rounded - rounded.round()).abs() < 1e-9 {
        format!("{}", rounded.round() as i64)
    } else {
        let mut s = format!("{rounded:.6}");
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

pub fn format_custom_table_status(lang: UiLanguage, state: &CustomTableState) -> (bool, String) {
    match state {
        CustomTableState::NotWritten => (false, tr(lang, "Not in the properties file").to_owned()),
        CustomTableState::Invalid(err) => {
            let key = match err {
                CustomTableError::MissingPair => {
                    "Not used: customAccelThreshold and customAccelMultiplier are both needed"
                }
                CustomTableError::ParseError => "Not used: a value is not a number",
                CustomTableError::LengthMismatch => {
                    "Not used: the numbers of thresholds and multipliers differ"
                }
                CustomTableError::InvalidThresholds => {
                    "Not used: the thresholds must be ascending numbers of 0 or more"
                }
                CustomTableError::InvalidMultipliers => {
                    "Not used: the multipliers must be numbers of 0 or more"
                }
            };
            (true, tr(lang, key).to_owned())
        }
        CustomTableState::Loaded {
            thresholds,
            multipliers,
        } => {
            let count = thresholds.len().to_string();
            let t_first = thresholds.first().copied().unwrap_or(0).to_string();
            let t_last = thresholds.last().copied().unwrap_or(0).to_string();
            let m_first = format_multiplier(multipliers.first().copied().unwrap_or(0.0));
            let m_last = format_multiplier(multipliers.last().copied().unwrap_or(0.0));
            let template = match lang {
                UiLanguage::En => {
                    "Loaded: {0} steps (thresholds {1} ... {2}, multipliers {3} ... {4})"
                }
                UiLanguage::Ja => {
                    "読み込み済み: {0} 段階 (閾値 {1} ... {2}、倍率 {3} ... {4})"
                }
            };
            let text = template
                .replace("{0}", &count)
                .replace("{1}", &t_first)
                .replace("{2}", &t_last)
                .replace("{3}", &m_first)
                .replace("{4}", &m_last);
            (false, text)
        }
    }
}

pub fn format_non_exe_warning(lang: UiLanguage, rejected: &[String]) -> String {
    let header = tr(lang, "These files are not programs (.exe): they were not added");
    let shown = 10usize;
    let mut lines: Vec<String> = rejected.iter().take(shown).cloned().collect();
    if rejected.len() > shown {
        let more = (rejected.len() - shown).to_string();
        let tmpl = match lang {
            UiLanguage::En => "... and {0} more",
            UiLanguage::Ja => "... 他 {0} 件",
        };
        lines.push(tmpl.replace("{0}", &more));
    }
    format!("{header}\n\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tooltip_formats_profile_and_respects_utf16_limit() {
        assert_eq!(
            format_tray_tooltip(UiLanguage::En, false, "Default"),
            "W10Wheel - Runnable"
        );
        assert_eq!(
            format_tray_tooltip(UiLanguage::Ja, true, "Gaming"),
            "W10Wheel - 停止中 (Gaming)"
        );

        let long_profile = "あ".repeat(130);
        let clamped = format_tray_tooltip(UiLanguage::Ja, false, &long_profile);
        assert!(clamped.ends_with("..."));
        assert!(clamped.encode_utf16().count() <= 127);
    }

    #[test]
    fn custom_table_status_formats_all_states_without_trailing_zero() {
        assert_eq!(
            format_custom_table_status(UiLanguage::En, &CustomTableState::NotWritten),
            (false, "Not in the properties file".to_owned())
        );
        assert_eq!(
            format_custom_table_status(
                UiLanguage::Ja,
                &CustomTableState::Invalid(CustomTableError::LengthMismatch)
            ),
            (
                true,
                "使われていません: 閾値と倍率の個数が違います".to_owned()
            )
        );
        assert_eq!(
            format_custom_table_status(
                UiLanguage::En,
                &CustomTableState::Loaded {
                    thresholds: vec![1, 5, 20],
                    multipliers: vec![1.0, 2.5, 4.0],
                }
            ),
            (
                false,
                "Loaded: 3 steps (thresholds 1 ... 20, multipliers 1 ... 4)".to_owned()
            )
        );
    }

    #[test]
    fn tooltip_and_non_exe_warning_preserve_keys_and_limit_rows() {
        let tip = format_tooltip(
            UiLanguage::Ja,
            "The priority of the process of W10Wheel",
            Some("processPriority"),
        );
        assert_eq!(tip, "W10Wheel のプロセスの優先度です\r\n(processPriority)");

        let files: Vec<String> = (0..12).map(|i| format!("C:\\doc{i}.txt")).collect();
        let warn = format_non_exe_warning(UiLanguage::En, &files);
        assert!(warn.contains("doc9.txt"));
        assert!(!warn.contains("doc10.txt"));
        assert!(warn.contains("... and 2 more"));
    }
}
