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
use regex::Regex;
use once_cell::sync::Lazy;

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
    id: i64,
    url: String,
    title: String,
    site_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Site {
    id: i64,
    name: String,
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Tag {
    id: i64,
    name: String,
    parent_id: Option<i64>,
    created_at: String,
}

// フロントエンド用（結果）
#[derive(Debug, Serialize, Deserialize)]
struct ArticleWithDetails {
    id: i64,
    url: String,
    title: String,
    site_name: Option<String>,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchFilters {
    tag_query: Option<String>,
    site: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SaveArticleRequest {
    url: String,
    title: String,
    tags: Option<String>,
}

// "よく使う"タグ管理用
#[derive(Debug, Serialize, Deserialize)]
struct TagCount {
    tag: String,
    count: u32,
}

struct AppState {
    db: Mutex<Connection>,
}

// 自動タグ付け用正規表現
// 正規表現を一度だけコンパイル（プログラム起動時）
static PREFIX_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(www\.|m\.|mobile\.|app\.|beta\.|dev\.|staging\.|blog\.|news\.|support\.|help\.|doc\.|api\.|cdn\.|static\.|shop\.|jp\.)").unwrap()
});

static COMPOUND_TLD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\.(co|com|ac|or|ne|go|ed|gov)\.[a-z]{2,3}$").unwrap()
});

static TLD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\.(com|net|org|edu|gov|mil|int|info|biz|name|pro|jp|us|uk|de|fr|ca|au|cn|kr|in|br|ru|it|es|io|ai|co|me|tv|cc|ly|app|dev|tech|blog|news|shop)$").unwrap()
});


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
        ) -> Result<Vec<ArticleWithDetails>, String>
{
    // 記事検索
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    let mut query = 
    "SELECT 
        a.id,
        a.url,
        a.title,
        COALESCE(s.name, '') as site_name,
        GROUP_CONCAT(t.name) as tags,
        a.created_at,
        a.updated_at
     FROM articles a
     LEFT JOIN sites s ON a.site_id = s.id
     LEFT JOIN article_tags at ON a.id = at.article_id
     LEFT JOIN tags t ON at.tag_id = t.id
     
    ".to_string();

    let mut conditions = Vec::new();
    let mut params: Vec<String> = Vec::new();
    
    
    if let Some(filters) = filters {
        // フィルター処理：サイト
        if let Some(site) = filters.site {
            conditions.push("s.name LIKE ?".to_string());
            params.push(format!("%{}%", site));
        }
        // フィルター処理：タグ
        if let Some(tag_query) = filters.tag_query{
            let search_tags : Vec<&str> = tag_query.split(',').map(|t| t.trim()).collect();
            for tag in search_tags{
                conditions.push("t.name = ? COLLATE NOCASE".to_string());
                params.push(tag.to_string());
            }
        }
    }
 
    
    if !conditions.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&conditions.join(" AND "));
    }
    
    query.push_str(" GROUP BY a.id, a.url, a.title, s.name, a.created_at, a.updated_at ");
    query.push_str(" ORDER BY updated_at DESC");
    
    let mut stmt = db.prepare(&query).map_err(|e| e.to_string())?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
    
    let articles = stmt.query_map(&param_refs[..], |row| {
        let tags_str: Option<String> = row.get(4)?;
        let tags = if let Some(tags_str) = tags_str {
            tags_str.split(',').map(|tag| tag.trim().to_string()).collect()
        } else {
            Vec::new()
        };        
        Ok(ArticleWithDetails  {
            id: row.get(0)?,
            url: row.get(1)?,
            title: row.get(2)?,
            site_name: Some(row.get(3)?),
            tags,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
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

    let db = state.db.lock().map_err(|e| e.to_string())?;

    let normalized_url = normalize_url(&request.url);
    let parsed_url = Url::parse(&normalized_url).map_err(|e| e.to_string())?;
    let site_name = parsed_url.host_str().unwrap_or("").replace("www.", "");
    // ph.1 サイトID確定
    let site_id = get_or_create_site(&db, &site_name)?;

    // ph.2 記事の保存
    println!("記事保存開始：{}", request.url);
    db.execute(
        "INSERT INTO articles (url, title, site_id, created_at, updated_at) 
         VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        params![normalized_url, request.title, site_id],
    ).map_err(|e| e.to_string())?;

    let article_id = db.last_insert_rowid();
    println!("記事作成完了: {} (ID: {})", request.title, article_id);

    // ph.3 タグの処理
    if let Some(tags_str) = request.tags {
        let tag_names: Vec<&str> = tags_str.split(',').map(|tag| tag.trim()).collect();
        for tag_name in tag_names {
            println!("処理中のタグ: '{}'", tag_name);
            if !tag_name.is_empty() {
                // 1. タグID取得/作成
                let tag_id = get_or_create_tag(&db, tag_name)?;
                
                // 2. 記事-タグ関連を作成
                println!("article_tags への INSERT: article_id={}, tag_id={}", article_id, tag_id);
                
                match db.execute(
                    "INSERT INTO article_tags (article_id, tag_id) VALUES (?, ?)",
                    params![article_id, tag_id]
                ) {
                    Ok(rows) => println!("article_tags INSERT 成功: {} rows", rows),
                    Err(e) => println!("article_tags INSERT エラー: {}", e),
                }
            }
        }
    } else{
        println!("タグなし（None）");
    }

    println!("記事保存完了");
    Ok("created".to_string())
}

#[tauri::command]
fn update_article(state: State<AppState>, request: SaveArticleRequest) -> Result<(), String> {
    println!("記事編集開始: {}", request.url);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    
    // ph.1 更新対象の記事ID特定
    let article_id = get_article_id_by_url(&db, &request.url)?;

    // ph.2 記事の基本情報を更新
    db.execute(
        "UPDATE articles SET title = ?, url = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        params![request.title, request.url, article_id]
    ).map_err(|e| e.to_string())?;

    println!("記事基本情報更新完了");

    // ph.3 既存のタグ-記事リレーションを削除
    db.execute(
        "DELETE FROM article_tags WHERE article_id = ?",
        params![article_id]
    ).map_err(|e| e.to_string())?;
    println!("既存タグ関連削除完了");

    // ph.4 タグ-記事リレーションを改めて登録
    if let Some(tags_str) = request.tags {
        let tag_names: Vec<&str> = tags_str.split(',').map(|tag| tag.trim()).collect();
        for tag_name in tag_names {
            if !tag_name.is_empty() {
                let tag_id = get_or_create_tag(&db, tag_name)?;
                db.execute(
                    "INSERT INTO article_tags (article_id, tag_id) VALUES (?, ?)",
                    params![article_id, tag_id]
                ).map_err(|e| e.to_string())?;
            }
        }
        println!("新しいタグ登録完了");
    }
    println!("記事編集完了");    
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
    
    // タグ自動生成
    let auto_tags = auto_tagging(browser_info.url.clone());
    println!("生成されたタグ: {}", auto_tags);
    
    let request = SaveArticleRequest {
        url: browser_info.url,
        title: browser_info.title,
        tags: Some(auto_tags),
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

    let mut stmt = db.prepare(
        "SELECT
           TRIM(t.name) as tag_name
         , COUNT(*) as count
        FROM tags t
        JOIN article_tags at
          ON t.id = at.tag_id
        WHERE t.name != 'auto-saved' --暫定
        GROUP BY TRIM(t.name) 
        ORDER BY count DESC, t.name ASC
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

//================================================================================================
// コマンド関連ファンクション等 - Functions and Sub procedures for command actions
//================================================================================================

// URL正規化（クエリパラメータ殺し）
fn normalize_url(url: &str) -> String {
    if url.starts_with("file://") {
        // ローカルファイルの場合はそのまま返す
        return url.to_string();
    }
    
    match Url::parse(url) {
        Ok(parsed_url) => {
            let host = parsed_url.host_str().unwrap_or("");
            
            // 一部のサイトではクエリパラメータを殺さない
            if url_preserve_targets(host) {
                return url.to_string();
            }
            format!("{}{}", parsed_url.origin().ascii_serialization(), parsed_url.path())
        }
        Err(_) => url.to_string(),
    }
}

// 登録サイトIDの特定
fn get_or_create_site(db: &Connection, site_name: &str) -> Result<i64, String> {
    // 登録済みサイトの検索（重複確認）
    let mut stmt = db.prepare("SELECT id FROM sites WHERE name = ?")
        .map_err(|e| e.to_string())?;

    let site_id_opt = stmt.query_row([site_name],
         |row| row.get(0)).optional().map_err(|e| e.to_string())?;
    if let Some(site_id) = site_id_opt {
        println!("既存サイト使用: {} (ID: {})", site_name, site_id);
        Ok(site_id)
    } else {
        // 新しいサイトを作成（INSERT）
        db.execute(
            "INSERT INTO sites (name) VALUES (?)",
            params![site_name],
        ).map_err(|e| e.to_string())?;

        // 作成したサイトのIDを取得
        let site_id = db.last_insert_rowid();
        println!("新規サイト作成: {} (ID: {})", site_name, site_id);
        Ok(site_id)
    }
}

// 登録に使うタグの特定
fn get_or_create_tag(db: &Connection, tag_name: &str) -> Result<i64, String> {
    let mut stmt = db.prepare("SELECT id FROM tags WHERE name = ?")
        .map_err(|e| e.to_string())?;
    
    let tag_id_opt = stmt.query_row([tag_name], |row| row.get(0)).optional().map_err(|e| e.to_string())?;
    
    if let Some(tag_id) = tag_id_opt {
        println!("既存タグ使用: {} (ID: {})", tag_name, tag_id);
        Ok(tag_id)
    } else {
        // 新しいタグを作成
        db.execute(
            "INSERT INTO tags (name) VALUES (?)",
            [tag_name]
        ).map_err(|e| e.to_string())?;
        
        let tag_id = db.last_insert_rowid();
        println!("新規タグ作成: {} (ID: {})", tag_name, tag_id);
        Ok(tag_id)
    }
}

// 記事IDをurlから求める
fn get_article_id_by_url(db: &Connection, url: &str) -> Result<i64, String> {
    let mut stmt = db.prepare("SELECT id FROM articles WHERE url = ?")
        .map_err(|e| e.to_string())?;
    
    stmt.query_row([url], |row| Ok(row.get(0)?))
        .map_err(|e| e.to_string())
}

fn url_preserve_targets(host: &str) -> bool {
    //TODO 将来的には設定ファイルから読み込みたい
    let url_preserve_sites = [
        "youtube.com",
        // あと何かあれば随時
    ];
    
    url_preserve_sites.iter().any(|&site| host.contains(site))
}

fn init_database() -> Result<Connection> {
    let conn = Connection::open("atode.db")?;

    conn.execute("PRAGMA foreign_keys = ON;", [])?;

    let ddl_files = [
        include_str!("ddl/001_create_sites.sql"),
        include_str!("ddl/002_create_articles.sql"),
        include_str!("ddl/003_create_tags.sql"),
        include_str!("ddl/004_create_article_tags.sql")
    ];

    for ddl in ddl_files.iter() {
        conn.execute(ddl, [])?;
    }
   
    Ok(conn)
}

// 自動タグ付け
fn auto_tagging(url: String) -> String{
    let mut tags: Vec<String> = Vec::with_capacity(3);
    
    // URLクレートでサイト名を抽出
    if let Ok(parsed_url) = Url::parse(&url){
        if let Some(host) = parsed_url.host_str(){
            //ホスト名クリーンアップ（よくあるprefix, suffix排除）
            let clean_site = clean_hostname(host);
            
            if clean_site.len() > 1 {
                // サイト名のタグを追加
                tags.push(clean_site.clone());
                // 推奨タグの自動付与
                add_essential_tags(&mut tags, &clean_site);
            }
        
        }
    }
    // 空の場合は空文字を返す
    if tags.is_empty() {
        return String::new();
    }
    
    // 重複削除とソート
    tags.sort_unstable();
    tags.dedup();
    
    tags.join(", ")
}

fn clean_hostname(host: &str) -> String {
    let mut input = host.to_lowercase();
    
    // prefix(www.など）除去
    if let Some(captures) = PREFIX_RE.find(&input){
        input.drain(0..captures.end());
    }
    
    // suffix除去（co.jpみたいなの→.comみたいなのの順）
    if let Some(captures) = COMPOUND_TLD_RE.find(&input) {
        input.truncate(captures.start());
    } else if let Some(captures) = TLD_RE.find(&input) {
        input.truncate(captures.start());
    }
    
    input
  
}

#[inline]
fn add_essential_tags(tags: &mut Vec<String>, site: &str) {
    match site {
        // AI系
        "claude" | "chatgpt" | "openai" | "anthropic" | "gemini"  => {
            tags.push("ai".to_string());
        }
        
        // プログラミング系
        "github" | "gitlab" | "stackoverflow" | "qiita" | "zenn" => {
            tags.push("programming".to_string());
        }
        
        // リファレンス系
        "wikipedia" | "mdn" => {
            tags.push("reference".to_string());
        }
        
        // 動画系
        "youtube" | "vimeo" | "twitch" => {
            tags.push("video".to_string());
        }
        
        // ソーシャル系
        "twitter" | "x" | "reddit" | "facebook" => {
            tags.push("social".to_string());
        }
        
        // ショッピング系
        "amazon" | "rakuten" => {
            tags.push("shopping".to_string());
        }
        
        // その他は追加タグなし（サイト名だけで十分）
        _ => {}
    }
}

//================================================================================================
// tests
//================================================================================================
#[cfg(test)]
mod tests{
    use super::*;
    
    #[test]
    fn test_url_normalization(){
        // URL正規化テスト（通常）
        assert_eq!(
            normalize_url("https://example.com/page?ref=123"),
            "https://example.com/page"
        );
        assert_eq!(
            normalize_url("https://github.com/user/repo"),
            "https://github.com/user/repo"
        );
        
	    // 特別扱い（GETパラメータを殺さない）ケース
        assert_eq!(
            normalize_url("https://www.youtube.com/watch?v=a1b2c3d4e5"),
            "https://www.youtube.com/watch?v=a1b2c3d4e5"
        );
    
        // ローカルファイル
        assert_eq!(
            normalize_url("file:///C:/Users/test/document.html"),
            "file:///C:/Users/test/document.html"
        );
    }
       
    #[test]
    fn test_auto_tagging(){
        assert_eq!(
            // prefix, suffix除去（単独）
            auto_tagging("https://www.sample.com".to_string()),
            "sample"
        );
        
        assert_eq!(
            // suffix除去（複合）
            auto_tagging("https://www.example.co.jp".to_string()),
            "example"
        );
        
        assert_eq!(
            // 推奨タグ
            auto_tagging("https://github.com/user".to_string()),
            "github, programming"
        )
    }
}