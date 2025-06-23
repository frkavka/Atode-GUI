//================================================================================================
// 依存関係 - Import Section
//================================================================================================
use tauri::{
            // アプリケーション状態管理
            State, AppHandle, Manager,
            // システムトレイ関連
            SystemTray, SystemTrayMenu, SystemTrayMenuItem, CustomMenuItem, SystemTrayEvent,
            // ショートカット関連
            GlobalShortcutManager,
};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
use rusqlite::{params, Connection, Result, OptionalExtension};
use url::Url;

// ブラウザ情報取得モジュール
mod browser;
use browser::{BrowserInfo, get_active_browser_info};

//================================================================================================
// データ構造・モジュール変数等 - Data Types & Module Variables
//================================================================================================

// グローバルなリフレッシュフラグ
static REFRESH_NEEDED: AtomicBool = AtomicBool::new(false);

// main
#[derive(Debug, Serialize, Deserialize)]
struct Article {
    url: String,
    title: String,
    site: String,
    tags: Option<String>,
    updated_at: String,
}

// 新規登録と更新は同じ構造だが、分けたくなったときのために別にしておく
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

// "よく使う"タグ管理用
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

//================================================================================================
// メイン処理 - Main Procedure
//================================================================================================

fn main() {
    let db = init_database().expect("DB初期化失敗");
    let system_tray = create_system_tray();
    
    tauri::Builder::<tauri::Wry>::new()
        .manage(AppState { db: Mutex::new(db) })
        .system_tray(system_tray)
        .on_system_tray_event(handle_system_tray_event)
        .invoke_handler(tauri::generate_handler![
            // 記事管理
            get_articles,
            save_article,
            update_article,
            delete_article,
            
            // システム操作
            open_url,
            save_active_page,
            check_refresh_needed,
            
            // UX強化用
            get_popular_tags,
            get_popular_sites,
        ])
        .setup(setup_application)
        .on_window_event(handle_window_event)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

//================================================================================================
// アプリ設定関連ファンクション等 - Functions and Sub procedures for application
//================================================================================================

fn setup_application(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>>{
    // アプリ起動時セットアップ
    
    let app_handle = app.handle();
    let app_handle2 = app.handle();
    let mut shortcut_manager = app.global_shortcut_manager();
    
    // ショートカット登録
    // クイック登録用
    shortcut_manager
        .register("Ctrl+Shift+S", move || {
            println!("Ctrl+Shift+S が押されました！");
            let app_state = app_handle.state::<AppState>();
            match save_active_page(app_state) {
                Ok(result) => println!("ショートカットでページを保存しました: {}", result),
                Err(e)     => eprintln!("ショートカット保存エラー: {}", e),
            }
        })
        .unwrap_or_else(|e| eprintln!("ショートカット登録エラー: {}", e));

    // アプリ表示/非表示用
    shortcut_manager
        .register("Ctrl+Shift+A", move || {
            if let Some(window) = app_handle2.get_window("main") {
                if window.is_visible().unwrap_or(false) {
                    window.hide().unwrap();
                } else {
                    window.show().unwrap();
                    window.unminimize().unwrap(); // 最小化解除
                    window.set_focus().unwrap();
                    window.set_always_on_top(true).unwrap();
                    window.set_always_on_top(false).unwrap(); // すぐ解除
                }
            }
        })
        .unwrap_or_else(|e| eprintln!("ショートカット登録エラー: {}", e));

    Ok(())
}

fn handle_window_event(event: tauri::GlobalWindowEvent) {
    // ウィンドウまわり制御
    
    match event.event() {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // ウィンドウを閉じる代わりに隠す
            event.window().hide().unwrap();
            api.prevent_close();
        }
        _ => {}
    }
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

//================================================================================================
// Tauri用コマンド設定 - Commands for Tauri
//================================================================================================

// TODO SQLを組み立てているところはDiesel等を使うようにする

// リフレッシュが必要かチェックするコマンド
#[tauri::command]
fn check_refresh_needed() -> Result<bool, String> {
    // 更新時に発火
    let needed = REFRESH_NEEDED.load(Ordering::Relaxed);
    if needed {
        REFRESH_NEEDED.store(false, Ordering::Relaxed);
        println!("✅ リフレッシュフラグをクリアしました");
    }
    Ok(needed)
}

#[tauri::command]
fn get_articles(
           state: State<AppState>, filters: Option<SearchFilters>
        ) -> Result<Vec<Article>, String>
{
    // 記事検索
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    let mut query = "SELECT url, title, site, tags, updated_at FROM articles".to_string();
    let mut conditions = Vec::new();
    let mut params: Vec<String> = Vec::new();
    
    // TODO タグテーブルを作ったら修正を入れる
    if let Some(filters) = filters {
        if let Some(tag_query) = filters.tag_query {
            let search_tags: Vec<&str> = tag_query.split(',').map(|t| t.trim()).collect();
            for tag in search_tags {
                conditions.push("COALESCE(tags, '') LIKE ?".to_string());
                params.push(format!("%{}%", tag)); // 単純な部分一致
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

// 人気タグを取得
#[tauri::command]
fn get_popular_tags(state: State<AppState>, limit: Option<usize>) -> Result<Vec<TagCount>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(20);
    
    // TODO タグのテーブルを作って正規化する（汚いので）
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

//================================================================================================
// コマンド関連ファンクション等 - Functions and Sub procedures for command actions
//================================================================================================


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
