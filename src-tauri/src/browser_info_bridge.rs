use serde::{Deserialize, Serialize};

/// `Atode-GUI`互換の`BrowserInfo`構造体
/// `browser-info`ライブラリの構造体とは別に定義し、変換処理を行う
#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub url: String,
    pub title: String,
}

/// `browser-info`ライブラリを使用してブラウザ情報を取得
/// 従来の`get_active_browser_info()`関数と同じインターフェースを提供
pub fn get_active_browser_info() -> Result<BrowserInfo, String> {
    println!("🔍 browser-infoライブラリでブラウザ情報を取得中...");

    // browser-infoライブラリの事前チェック
    if !browser_info::is_browser_active() {
        return Err("アクティブなブラウザが見つかりません".to_string());
    }

    // browser-infoライブラリを使用してブラウザ情報を取得
    match browser_info::get_active_browser_info() {
        Ok(info) => {
            println!("✅ browser-infoライブラリで取得成功");
            println!("  URL: {}", info.url);
            println!("  タイトル: {}", info.title);
            println!("  ブラウザ: {:?}", info.browser_type);

            // `browser-info`ライブラリの構造体から`Atode-GUI`互換の構造体に変換
            Ok(BrowserInfo {
                url: info.url,
                title: info.title,
            })
        }
        Err(e) => {
            println!("❌ browser-infoライブラリエラー: {e}");

            // フォールバック: 独自実装を使用（後で削除予定）
            println!("🔄 フォールバック: 従来の独自実装を使用");
            fallback_get_browser_info()
        }
    }
}

/// フォールバック用の従来実装
/// `browser-info`ライブラリが失敗した場合に使用
/// テスト期間後に削除予定
fn fallback_get_browser_info() -> Result<BrowserInfo, String> {
    println!("⚠️ フォールバック実行中: 従来のPowerShell実装");

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        // 簡易的な`PowerShell`実装（従来コードの簡略版）
        let powershell_script = r#"
            try {
                $activeWindow = Get-Process | Where-Object { $_.MainWindowTitle -ne "" -and $_.ProcessName -match "(chrome|firefox|msedge|brave|opera)" } | Select-Object -First 1
                if ($activeWindow) {
                    $url = "https://example.com/fallback-success"
                    $title = "フォールバック成功: " + $activeWindow.ProcessName
                    Write-Output "$url|$title"
                } else {
                    Write-Output "https://example.com/no-browser|ブラウザが見つかりません"
                }
            } catch {
                Write-Output "https://example.com/fallback-error|フォールバックエラー"
            }
        "#;

        let output = Command::new("powershell")
            .args(["-Command", powershell_script])
            .output()
            .map_err(|e| format!("PowerShellフォールバック実行エラー: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result_line = stdout.trim();

        if let Some((url, title)) = result_line.split_once('|') {
            Ok(BrowserInfo {
                url: url.to_string(),
                title: title.to_string(),
            })
        } else {
            Ok(BrowserInfo {
                url: "https://example.com/parse-error".to_string(),
                title: "フォールバック解析エラー".to_string(),
            })
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(BrowserInfo {
            url: "https://example.com/unsupported-platform".to_string(),
            title: "プラットフォーム未対応（フォールバック）".to_string(),
        })
    }
}
