use serde::{Deserialize, Serialize};
use std::process::Command;
use tauri::api::path::resource_dir;

#[derive(Debug, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub url: String,
    pub title: String,
}

/// プラットフォーム固有のブラウザ情報取得
pub fn get_active_browser_info() -> Result<BrowserInfo, String> {
    #[cfg(target_os = "windows")]
    {
        get_windows_browser_info()
    }
    
    #[cfg(target_os = "macos")]
    {
        get_macos_browser_info()
    }
    
    #[cfg(target_os = "linux")]
    {
        get_linux_browser_info()
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("サポートされていないプラットフォームです".to_string())
    }
}

#[cfg(target_os = "windows")]
fn get_windows_browser_info() -> Result<BrowserInfo, String> {
    println!("🔍 Windows環境でブラウザ情報を取得中...");
    
    // PowerShellスクリプトファイルのパスを取得
    let script_path = get_script_path("windows_get_url.ps1")?;
    
    let output = Command::new("powershell")
        .args([
            "-ExecutionPolicy", "Bypass",
            "-NoProfile",
            "-File", &script_path
        ])
        .output()
        .map_err(|e| format!("PowerShell実行エラー: {}", e))?;
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8(output.stdout.clone())
        .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).to_string());
    
    println!("PowerShell stderr: {}", stderr);
    
    if !output.status.success() {
        return Ok(BrowserInfo {
            url: "https://example.com/powershell-error".to_string(),
            title: "PowerShell実行エラー".to_string(),
        });
    }
    
    parse_browser_output(&stdout)
}

#[cfg(target_os = "macos")]
fn get_macos_browser_info() -> Result<BrowserInfo, String> {
    println!("🔍 macOS環境でブラウザ情報を取得中...");
    
    // 将来的にAppleScriptやJXAを使用
    // let script_path = get_script_path("macos_get_url.sh")?;
    
    // 現在は未実装
    Ok(BrowserInfo {
        url: "https://example.com/macos-not-implemented".to_string(),
        title: "macOS対応準備中".to_string(),
    })
}

#[cfg(target_os = "linux")]
fn get_linux_browser_info() -> Result<BrowserInfo, String> {
    println!("🔍 Linux環境でブラウザ情報を取得中...");
    
    // 将来的にxdotool等を使用
    // let script_path = get_script_path("linux_get_url.sh")?;
    
    // 現在は未実装
    Ok(BrowserInfo {
        url: "https://example.com/linux-not-implemented".to_string(),
        title: "Linux対応準備中".to_string(),
    })
}

/// スクリプトファイルのパスを取得
fn get_script_path(filename: &str) -> Result<String, String> {

    // Tauri contextを生成
    let context = tauri::generate_context!();
    let package_info = context.package_info();
    let env = tauri::Env::default();

   // リソースディレクトリを取得
    let resource_dir_path = resource_dir(&package_info, &env)
        .ok_or("リソースディレクトリが取得できません")?;

    // 複数のパスパターンを試す  // ← 新しいロジック
    let possible_paths = [
        resource_dir_path.join("src").join("scripts").join(filename),
        resource_dir_path.join("scripts").join(filename),
        resource_dir_path.join(filename),
    ];
    
    for path in &possible_paths {
        if path.exists() {
            // パスを文字列として返す
            return path.to_str()
                .ok_or("パスを文字列に変換できません".to_string())
                .map(|s| s.to_string());
        }
    }
    
    Err(format!("スクリプトファイルが見つかりません: {}", filename))
}

/// ブラウザ出力を解析してBrowserInfoに変換
fn parse_browser_output(output: &str) -> Result<BrowserInfo, String> {
    // 最後の行を取得（結果のみ）
    let lines: Vec<&str> = output.lines().collect();
    let result_line = lines.iter().rev().find(|line| line.contains("|")).unwrap_or(&"").trim();
    
    if result_line.is_empty() || result_line.starts_with("ERROR") {
        return Ok(BrowserInfo {
            url: "https://example.com/no-result".to_string(),
            title: "結果取得失敗".to_string(),
        });
    }
    
    let parts: Vec<&str> = result_line.split('|').collect();
    
    if parts.len() >= 2 {
        let url = parts[0].trim().to_string();
        let title = parts[1].trim().to_string();
        let process_name = parts.get(2).unwrap_or(&"unknown").trim().to_string();
        
        println!("✅ ブラウザ情報取得成功");
        println!("  URL: {}", url);
        println!("  タイトル: {}", title);
        println!("  プロセス: {}", process_name);
        
        Ok(BrowserInfo { url, title })
    } else {
        Ok(BrowserInfo {
            url: "https://example.com/parse-error".to_string(),
            title: "解析エラー".to_string(),
        })
    }
}