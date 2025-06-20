use tauri::{State, SystemTray, SystemTrayMenu, SystemTrayMenuItem, CustomMenuItem, SystemTrayEvent, AppHandle, Manager, GlobalShortcutManager};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
use rusqlite::{params, Connection, Result, OptionalExtension};
use url::Url;

// グローバルなリフレッシュフラグ
static REFRESH_NEEDED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, Deserialize)]
struct Article {
    url: String,
    title: String,
    site: String,
    tags: Option<String>,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SaveArticleRequest {
    url: String,
    title: String,
    tags: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateArticleRequest {
    url: String,
    title: String,
    tags: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchFilters {
    tag_query: Option<String>,
    site: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BrowserInfo {
    url: String,
    title: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TagCount {
    tag: String,
    count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SiteCount {
    site: String,
    count: u32,
}

struct AppState {
    db: Mutex<Connection>,
}

// リフレッシュが必要かチェックするコマンド
#[tauri::command]
fn check_refresh_needed() -> Result<bool, String> {
    let needed = REFRESH_NEEDED.load(Ordering::Relaxed);
    if needed {
        REFRESH_NEEDED.store(false, Ordering::Relaxed);
        println!("✅ リフレッシュフラグをクリアしました");
    }
    Ok(needed)
}

fn normalize_url(url: &str) -> String {
    if url.starts_with("file://") {
        // ローカルファイルの場合はそのまま返す
        return url.to_string();
    }
    
    match Url::parse(url) {
        Ok(parsed_url) => {
            format!("{}{}", parsed_url.origin().ascii_serialization(), parsed_url.path())
        }
        Err(_) => url.to_string(),
    }
}

fn init_database() -> Result<Connection> {
    let conn = Connection::open("atode.db")?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS articles (
            url TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            site TEXT,
            tags TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    
    Ok(conn)
}

// Windows用のブラウザ情報取得（キーボードショートカット版）
#[cfg(target_os = "windows")]
fn get_active_browser_info() -> Result<BrowserInfo, String> {
    use std::process::Command;
    
    println!("🔍 キーボードショートカットでアドレスバー情報を取得中...");
    
    let script = r#"
        # UTF-8エンコーディングを設定
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $OutputEncoding = [System.Text.Encoding]::UTF8
        
        Add-Type -TypeDefinition @"
            using System;
            using System.Runtime.InteropServices;
            using System.Text;
            using System.Threading;
            
            public class Win32API {
                [DllImport("user32.dll", CharSet = CharSet.Unicode)]
                public static extern IntPtr GetForegroundWindow();
                
                [DllImport("user32.dll", CharSet = CharSet.Unicode)]
                public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
                
                [DllImport("user32.dll")]
                public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
                
                [DllImport("user32.dll")]
                public static extern bool SetForegroundWindow(IntPtr hWnd);
                
                [DllImport("user32.dll")]
                public static extern void keybd_event(byte bVk, byte bScan, int dwFlags, int dwExtraInfo);
                
                [DllImport("user32.dll")]
                public static extern short GetKeyState(int nVirtKey);
                
                public const int KEYEVENTF_EXTENDEDKEY = 0x0001;
                public const int KEYEVENTF_KEYUP = 0x0002;
                public const byte VK_CONTROL = 0x11;
                public const byte VK_L = 0x4C;
                public const byte VK_C = 0x43;
                public const byte VK_ESCAPE = 0x1B;
            }
"@

        Add-Type -AssemblyName System.Windows.Forms

        function Send-KeyCombination {
            param([byte]$Key1, [byte]$Key2 = 0)
            
            # Ctrl キーを押下
            [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
            Start-Sleep -Milliseconds 50
            
            # 指定されたキーを押下
            [Win32API]::keybd_event($Key1, 0, 0, 0)
            Start-Sleep -Milliseconds 50
            
            if ($Key2 -ne 0) {
                [Win32API]::keybd_event($Key2, 0, 0, 0)
                Start-Sleep -Milliseconds 50
            }
            
            # キーを離す（逆順）
            if ($Key2 -ne 0) {
                [Win32API]::keybd_event($Key2, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
            }
            [Win32API]::keybd_event($Key1, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
            [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
            
            Start-Sleep -Milliseconds 100
        }

        function Get-URLFromAddressBar {
            param($ProcessName)
            
            try {
                Write-Host "🔍 アドレスバーからURL取得開始 ($ProcessName)" -ForegroundColor Yellow
                
                # 現在のクリップボードの内容を保存
                $originalClipboard = ""
                try {
                    $originalClipboard = [System.Windows.Forms.Clipboard]::GetText()
                } catch {
                    # クリップボードが空の場合は無視
                }
                
                $url = $null
                
                
                # 高速方法: F6 x2 (Vivaldiなどで効果的)
                Write-Host "  ⚡ 高速方法: F6 x2..." -ForegroundColor Cyan
                
                # F6を2回（高速)
                [Win32API]::keybd_event(0x75, 0, 0, 0)  # F6 key
                [Win32API]::keybd_event(0x75, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                Start-Sleep -Milliseconds 80
                [Win32API]::keybd_event(0x75, 0, 0, 0)  # F6 key again
                [Win32API]::keybd_event(0x75, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                Start-Sleep -Milliseconds 80
                
                # 高速全選択とコピー
                [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
                [Win32API]::keybd_event(0x41, 0, 0, 0)  # A key
                Start-Sleep -Milliseconds 100

                [Win32API]::keybd_event(0x41, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                Start-Sleep -Milliseconds 100

                # 高速コピー
                [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
                [Win32API]::keybd_event([Win32API]::VK_C, 0, 0, 0)
                Start-Sleep -Milliseconds 80
                
                [Win32API]::keybd_event([Win32API]::VK_C, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                Start-Sleep -Milliseconds 100
                
                try {
                    $url = [System.Windows.Forms.Clipboard]::GetText().Trim()
                    Write-Host "  📋 高速方法で取得: '$url'" -ForegroundColor Cyan
                    if ($url -and $url -match '^https?://' -or $url -match '^file://') {
                        Write-Host "  ✅ 高速方法成功: $url" -ForegroundColor Green
                    } else {
                        Write-Host "  ⚠️ 高速方法: URL不完全、フォールバックへ" -ForegroundColor Yellow
                        $url = $null
                    }
                } catch {
                    Write-Host "  ⚠️ 高速方法: クリップボードエラー、フォールバックへ" -ForegroundColor Yellow
                    $url = $null
                }
                
                # フォールバック: より確実だが低速な方法
                if (-not $url) {
                    Write-Host "  🐌 確実方法: Ctrl+L + 待機..." -ForegroundColor Cyan
                    
                    # より確実なCtrl+L
                    [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
                    [Win32API]::keybd_event([Win32API]::VK_L, 0, 0, 0)
                    Start-Sleep -Milliseconds 80
                    [Win32API]::keybd_event([Win32API]::VK_L, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                    [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                    Start-Sleep -Milliseconds 300  # 必要最小限の待機
                    
                    # 2回のCtrl+A（確実性のため）
                    for ($i = 0; $i -lt 2; $i++) {
                        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
                        [Win32API]::keybd_event(0x41, 0, 0, 0)  # A key
                        Start-Sleep -Milliseconds 80
                        [Win32API]::keybd_event(0x41, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                        Start-Sleep -Milliseconds 100
                    }
                    
                    # コピー
                    [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
                    [Win32API]::keybd_event([Win32API]::VK_C, 0, 0, 0)
                    Start-Sleep -Milliseconds 80
                    [Win32API]::keybd_event([Win32API]::VK_C, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                    [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                    Start-Sleep -Milliseconds 200
                    
                    try {
                        $url = [System.Windows.Forms.Clipboard]::GetText().Trim()
                        Write-Host "  📋 確実方法で取得: '$url'" -ForegroundColor Cyan
                        if ($url -and $url -match '^https?://' -or $url -match '^file://') {
                            Write-Host "  ✅ 確実方法成功: $url" -ForegroundColor Green
                        } else {
                            Write-Host "  ❌ 確実方法も失敗" -ForegroundColor Red
                            $url = $null
                        }
                    } catch {
                        Write-Host "  ❌ 確実方法: クリップボードエラー" -ForegroundColor Red
                        $url = $null
                    }
                }
                
                # 高速Escape
                [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, 0, 0)
                Start-Sleep -Milliseconds 30
                [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                
                # クリップボード復元
                try {
                    if ($originalClipboard) {
                        [System.Windows.Forms.Clipboard]::SetText($originalClipboard)
                    }
                } catch {
                    # 復元失敗は無視
                }
                
                if ($url -and $url -match '^https?://' -or $url -match '^file://') {
                    Write-Host "  🎉 最終URL: $url" -ForegroundColor Green
                    return $url
                } else {
                    Write-Host "  ❌ 全ての方法が失敗" -ForegroundColor Red
                    return $null
                }
                
            } catch {
                Write-Host "❌ URL取得エラー: $($_.Exception.Message)" -ForegroundColor Red
                return $null
            }
        }

        function Get-BrowserInfoKeyboard {
            try {
                $hwnd = [Win32API]::GetForegroundWindow()
                if ($hwnd -eq [IntPtr]::Zero) {
                    throw "WINDOW_NOT_FOUND"
                }
                
                # ウィンドウタイトルを取得
                $title = New-Object System.Text.StringBuilder 2048
                $titleLength = [Win32API]::GetWindowText($hwnd, $title, $title.Capacity)
                
                if ($titleLength -eq 0) {
                    throw "TITLE_NOT_FOUND"
                }
                
                $windowTitle = $title.ToString()
                
                # プロセス情報を取得
                $processId = 0
                [Win32API]::GetWindowThreadProcessId($hwnd, [ref]$processId) | Out-Null
                
                $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
                if (-not $process) {
                    throw "PROCESS_NOT_FOUND"
                }
                
                $processName = $process.ProcessName
                
                # ブラウザプロセスかチェック
                $browserProcesses = @("chrome", "firefox", "edge", "brave", "opera", "vivaldi", "msedge", "iexplore", "safari")
                if ($processName.ToLower() -notin $browserProcesses) {
                    # ブラウザ以外の場合は特別な形式で返す
                    Write-Output "NOT_BROWSER|$processName|not_browser"
                    return
                }
                
                Write-Host "✅ ブラウザプロセス検出: $processName" -ForegroundColor Green
                
                # ブラウザのタイトルをクリーンアップ
                $cleanTitle = $windowTitle
                switch ($processName.ToLower()) {
                    "chrome" { $cleanTitle = $cleanTitle -replace " - Google Chrome.*$", "" }
                    "firefox" { $cleanTitle = $cleanTitle -replace " — Mozilla Firefox.*$", "" -replace " - Mozilla Firefox.*$", "" }
                    "msedge" { $cleanTitle = $cleanTitle -replace " - Microsoft Edge.*$", "" }
                    "edge" { $cleanTitle = $cleanTitle -replace " - Microsoft Edge.*$", "" }
                    "brave" { $cleanTitle = $cleanTitle -replace " - Brave.*$", "" }
                    "opera" { $cleanTitle = $cleanTitle -replace " - Opera.*$", "" }
                    "vivaldi" { $cleanTitle = $cleanTitle -replace " - Vivaldi.*$", "" }
                }
                
                # ウィンドウがアクティブであることを確認
                [Win32API]::SetForegroundWindow($hwnd) | Out-Null
                Start-Sleep -Milliseconds 100
                
                # キーボードショートカットでURLを取得
                $actualUrl = Get-URLFromAddressBar -ProcessName $processName
                
                if ($actualUrl -and $actualUrl -match '^https?://' -or $actualUrl -match '^file://') {
                    Write-Host "🎉 アドレスバーからURL取得成功!" -ForegroundColor Green
                    $finalUrl = $actualUrl
                } else {
                    Write-Host "⚠️ アドレスバー取得失敗、タイトルから推測" -ForegroundColor Yellow
                    
                    # フォールバック: タイトルベースの推測
                    if ($windowTitle -match "Claude") {
                        $finalUrl = "https://claude.ai/chat"
                    } elseif ($windowTitle -match "GitHub.*?([a-zA-Z0-9_.-]+)/([a-zA-Z0-9_.-]+)") {
                        $finalUrl = "https://github.com/$($matches[1])/$($matches[2])"
                    } elseif ($windowTitle -match "GitHub") {
                        $finalUrl = "https://github.com"
                    } elseif ($windowTitle -match "Stack Overflow") {
                        $finalUrl = "https://stackoverflow.com"
                    } elseif ($windowTitle -match "YouTube") {
                        $finalUrl = "https://www.youtube.com"
                    } elseif ($windowTitle -match "Google") {
                        $finalUrl = "https://www.google.com"
                    } elseif ($windowTitle -match "Qiita") {
                        $finalUrl = "https://qiita.com"
                    } elseif ($windowTitle -match "Zenn") {
                        $finalUrl = "https://zenn.dev"
                    } else {
                        $finalUrl = "https://example.com/keyboard-method-failed"
                    }
                }
                
                # ローカルファイルの場合はタイトルに注釈を追加
                if ($finalUrl -match '^file://') {
                    //将来的なマルチデバイス対応を意識してデバイス名付きで提示
                    $hostName = $env:COMPUTERNAME
                    $cleanTitle = "[LocalFile：" + $hostName + "] " + $cleanTitle
                }
                
                # 結果のみ出力（デバッグ出力は標準エラーに送信）
                Write-Output "$finalUrl|$cleanTitle|$processName"
                
            } catch {
                Write-Output "ERROR|$($_.Exception.Message)|unknown"
            }
        }
        
        # 実行
        Get-BrowserInfoKeyboard
    "#;
    
    let output = Command::new("powershell")
        .args([
            "-ExecutionPolicy", "Bypass", 
            "-NoProfile",
            "-Command", script
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
    
    // 最後の行を取得（結果のみ）
    let lines: Vec<&str> = stdout.lines().collect();
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

// その他のOS用のプレースホルダー
#[cfg(not(target_os = "windows"))]
fn get_active_browser_info() -> Result<BrowserInfo, String> {
    Err("この機能はWindowsでのみ利用可能です".to_string())
}

#[tauri::command]
fn get_articles(state: State<AppState>, filters: Option<SearchFilters>) -> Result<Vec<Article>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    let mut query = "SELECT url, title, site, tags, updated_at FROM articles".to_string();
    let mut conditions = Vec::new();
    let mut params: Vec<String> = Vec::new();
    
    if let Some(filters) = filters {
        if let Some(tag_query) = filters.tag_query {
            let search_tags: Vec<&str> = tag_query.split(',').map(|t| t.trim()).collect();
            for tag in search_tags {
                conditions.push("(',' || COALESCE(tags, '') || ',') LIKE ?".to_string());
                params.push(format!("%,{},%", tag));
            }
        }
        
        if let Some(site) = filters.site {
            conditions.push("site LIKE ?".to_string());
            params.push(format!("%{}%", site));
        }
    }
    
    if !conditions.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&conditions.join(" AND "));
    }
    
    query.push_str(" ORDER BY updated_at DESC");
    
    let mut stmt = db.prepare(&query).map_err(|e| e.to_string())?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
    
    let articles = stmt.query_map(&param_refs[..], |row| {
        Ok(Article {
            url: row.get(0)?,
            title: row.get(1)?,
            site: row.get(2)?,
            tags: row.get(3)?,
            updated_at: row.get(4)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for article in articles {
        result.push(article.map_err(|e| e.to_string())?);
    }
    
    Ok(result)
}

#[tauri::command]
fn save_article(state: State<AppState>, request: SaveArticleRequest) -> Result<String, String> {
    println!("記事保存開始: {}", request.url);
    
    let db = state.db.lock().map_err(|e| {
        eprintln!("データベースロックエラー: {}", e);
        e.to_string()
    })?;
    
    let normalized_url = normalize_url(&request.url);
    let parsed_url = Url::parse(&normalized_url).map_err(|e| {
        eprintln!("URL解析エラー: {}", e);
        e.to_string()
    })?;
    let site = parsed_url.host_str().unwrap_or("").replace("www.", "");
    
    let mut stmt = db.prepare("SELECT title FROM articles WHERE url = ?").map_err(|e| {
        eprintln!("SQLクエリ準備エラー: {}", e);
        e.to_string()
    })?;
    let existing = stmt.query_row([&normalized_url], |_| Ok(())).optional().map_err(|e| {
        eprintln!("既存記事チェックエラー: {}", e);
        e.to_string()
    })?;
    
    let result = if existing.is_some() {
        let tags = request.tags.unwrap_or_default();
        db.execute(
            "UPDATE articles SET title = ?, tags = ?, updated_at = CURRENT_TIMESTAMP WHERE url = ?",
            params![request.title, tags, normalized_url],
        ).map_err(|e| {
            eprintln!("記事更新エラー: {}", e);
            e.to_string()
        })?;
        println!("記事を更新しました: {}", normalized_url);
        "updated".to_string()
    } else {
        let tags = request.tags.unwrap_or_default();
        db.execute(
            "INSERT INTO articles (url, title, site, tags) VALUES (?, ?, ?, ?)",
            params![normalized_url, request.title, site, tags],
        ).map_err(|e| {
            eprintln!("記事挿入エラー: {}", e);
            e.to_string()
        })?;
        println!("新しい記事を作成しました: {}", normalized_url);
        "created".to_string()
    };
    
    println!("記事保存完了: {}", result);
    Ok(result)
}

#[tauri::command]
fn update_article(state: State<AppState>, request: UpdateArticleRequest) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    let tags = request.tags.unwrap_or_default();
    db.execute(
        "UPDATE articles SET title = ?, tags = ?, updated_at = CURRENT_TIMESTAMP WHERE url = ?",
        params![request.title, tags, request.url],
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn delete_article(state: State<AppState>, url: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    db.execute("DELETE FROM articles WHERE url = ?", params![url])
        .map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    use std::process::Command;
    
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/c", "start", &url]).spawn().map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&url).spawn().map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&url).spawn().map_err(|e| e.to_string())?;
    }
    
    Ok(())
}


#[tauri::command]
fn save_active_page(state: State<AppState>) -> Result<String, String> {
    println!("自動保存開始...");
    
    let browser_info = match get_active_browser_info() {
        Ok(info) => info,
        Err(e) => {
            println!("ブラウザ情報取得失敗: {}, フォールバックを使用", e);
            BrowserInfo {
                url: "https://example.com/error-fallback".to_string(),
                title: "自動取得に失敗 - 手動で編集してください".to_string(),
            }
        }
    };
    
    let request = SaveArticleRequest {
        url: browser_info.url,
        title: browser_info.title,
        tags: Some("auto-saved".to_string()),
    };
    
    let result = save_article(state, request)?;
    
    println!("保存完了、リフレッシュフラグを設定...");
    
    // リフレッシュが必要であることをフラグで通知
    REFRESH_NEEDED.store(true, Ordering::Relaxed);
    println!("✅ リフレッシュフラグを設定しました");
    
    Ok(result)
}

// システムトレイの設定
fn create_system_tray() -> SystemTray {
    let show = CustomMenuItem::new("show".to_string(), "Atodeを表示");
    let save_page = CustomMenuItem::new("save_page".to_string(), "現在のページを保存");
    let quit = CustomMenuItem::new("quit".to_string(), "終了");
    
    let tray_menu = SystemTrayMenu::new()
        .add_item(show)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(save_page)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(quit);
    
    SystemTray::new().with_menu(tray_menu)
}

// システムトレイイベントの処理
fn handle_system_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            let window = app.get_window("main").unwrap();
            if window.is_visible().unwrap_or(false) {
                window.hide().unwrap();
            } else {
                window.show().unwrap();
                window.set_focus().unwrap();
            }
        }
        SystemTrayEvent::MenuItemClick { id, .. } => {
            match id.as_str() {
                "show" => {
                    let window = app.get_window("main").unwrap();
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
                "save_page" => {
                    // アクティブページ保存をバックグラウンドで実行
                    let app_state = app.state::<AppState>();
                    match save_active_page(app_state) {
                        Ok(result) => println!("トレイからページを保存しました: {}", result),
                        Err(e) => eprintln!("トレイからの保存エラー: {}", e),
                    }
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

// 人気タグを取得
#[tauri::command]
fn get_popular_tags(state: State<AppState>, limit: Option<usize>) -> Result<Vec<TagCount>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(20);
    
    let mut stmt = db.prepare(
        "SELECT 
            TRIM(value) as tag, 
            COUNT(*) as count 
         FROM articles, 
              json_each('[\"' || REPLACE(REPLACE(COALESCE(tags, ''), ',', '\",\"'), ' ', '') || '\"]')
         WHERE tags IS NOT NULL AND tags != '' AND TRIM(value) != ''
         GROUP BY TRIM(value) 
         ORDER BY count DESC, tag ASC
         LIMIT ?"
    ).map_err(|e| e.to_string())?;
    
    let tag_counts = stmt.query_map([limit], |row| {
        Ok(TagCount {
            tag: row.get(0)?,
            count: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for tag_count in tag_counts {
        result.push(tag_count.map_err(|e| e.to_string())?);
    }
    
    Ok(result)
}

// 人気サイトを取得
#[tauri::command]
fn get_popular_sites(state: State<AppState>, limit: Option<usize>) -> Result<Vec<SiteCount>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(10);
    
    let mut stmt = db.prepare(
        "SELECT site, COUNT(*) as count 
         FROM articles 
         WHERE site IS NOT NULL AND site != ''
         GROUP BY site 
         ORDER BY count DESC, site ASC
         LIMIT ?"
    ).map_err(|e| e.to_string())?;
    
    let site_counts = stmt.query_map([limit], |row| {
        Ok(SiteCount {
            site: row.get(0)?,
            count: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for site_count in site_counts {
        result.push(site_count.map_err(|e| e.to_string())?);
    }
    
    Ok(result)
}

fn main() {
    let db = init_database().expect("Failed to initialize database");
    
    let system_tray = create_system_tray();
    
    tauri::Builder::<tauri::Wry>::new()
        .manage(AppState {
            db: Mutex::new(db),
        })
        .system_tray(system_tray)
        .on_system_tray_event(handle_system_tray_event)
        .invoke_handler(tauri::generate_handler![
            get_articles,
            save_article,
            update_article,
            delete_article,
            open_url,
            save_active_page,
            check_refresh_needed,
            get_popular_tags,
            get_popular_sites
        ])
        .setup(|app| {
            // グローバルショートカットの設定
            let app_handle = app.handle();
            let app_handle2 = app.handle();
            
            // Ctrl+Shift+S でアクティブページ保存
            let mut shortcut_manager = app.global_shortcut_manager();
            shortcut_manager
                .register("Ctrl+Shift+S", move || {
                    println!("Ctrl+Shift+S が押されました！");
                    let app_state = app_handle.state::<AppState>();
                    match save_active_page(app_state) {
                        Ok(result) => println!("ショートカットでページを保存しました: {}", result),
                        Err(e) => eprintln!("ショートカット保存エラー: {}", e),
                    }
                })
                .unwrap_or_else(|e| eprintln!("ショートカット登録エラー: {}", e));
            
            // Ctrl+Shift+A でGUI表示/非表示切り替え
            shortcut_manager
                .register("Ctrl+Shift+A", move || {
                    if let Some(window) = app_handle2.get_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            window.hide().unwrap();
                        } else {
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        }
                    }
                })
                .unwrap_or_else(|e| eprintln!("ショートカット登録エラー: {}", e));
            
            Ok(())
        })
        .on_window_event(|event| {
            match event.event() {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // ウィンドウを閉じる代わりに隠す
                    event.window().hide().unwrap();
                    api.prevent_close();
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}