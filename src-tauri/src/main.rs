//================================================================================================
// 依存関係 - Import Section
//================================================================================================
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use tauri::{
    menu::{MenuBuilder, MenuItem},
    // Tauri 2.0 imports
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
    Manager,
    // アプリケーション状態管理
    State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// Win32 APIホットキー実装用
#[cfg(target_os = "windows")]
use std::{mem, ptr, thread};
use url::Url;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    DispatchMessageW, GetMessageW, PostQuitMessage, RegisterHotKey, TranslateMessage,
    UnregisterHotKey, MSG, WM_HOTKEY,
};

// ブラウザ情報取得モジュール
mod browser_info_bridge;
use browser_info_bridge::get_active_browser_info;

//================================================================================================
// データ構造・モジュール変数等 - Data Types & Module Variables
//================================================================================================

// グローバルなリフレッシュフラグ
static REFRESH_NEEDED: AtomicBool = AtomicBool::new(false);

// ホットキーデバウンス用のタイムスタンプ（ミリ秒）
static LAST_SAVE_HOTKEY: AtomicU64 = AtomicU64::new(0);
static LAST_TOGGLE_HOTKEY: AtomicU64 = AtomicU64::new(0);

// ホットキー管理状態（登録済みかどうか）
static HOTKEYS_REGISTERED: AtomicBool = AtomicBool::new(false);

// Win32ネイティブホットキー状態
static WIN32_HOTKEYS_ACTIVE: AtomicBool = AtomicBool::new(false);
static WIN32_HOTKEY_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);

// Win32ホットキーID
#[cfg(target_os = "windows")]
const HOTKEY_SAVE_ID: i32 = 1001;
#[cfg(target_os = "windows")]
const HOTKEY_TOGGLE_ID: i32 = 1002;

// Win32ホットキーコンバイネーション
#[cfg(target_os = "windows")]
const MOD_CTRL_SHIFT: u32 = 0x0002 | 0x0004; // MOD_CONTROL | MOD_SHIFT
#[cfg(target_os = "windows")]
const VK_S: u32 = 0x53;
#[cfg(target_os = "windows")]
const VK_A: u32 = 0x41;

// デバウンス間隔（ミリ秒）
const DEBOUNCE_MS: u64 = 500;

// 設定ファイル構造体
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    database_path: String,
}

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
static PREFIX_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(www\.|m\.|mobile\.|app\.|beta\.|dev\.|staging\.|blog\.|news\.|support\.|help\.|doc\.|api\.|cdn\.|static\.|shop\.|jp\.)").unwrap()
});

static COMPOUND_TLD_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\.(co|com|ac|or|ne|go|ed|gov)\.[a-z]{2,3}$").unwrap());

static TLD_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\.(com|net|org|edu|gov|mil|int|info|biz|name|pro|jp|us|uk|de|fr|ca|au|cn|kr|in|br|ru|it|es|io|ai|co|me|tv|cc|ly|app|dev|tech|blog|news|shop)$").unwrap()
});

//================================================================================================
// メイン処理 - Main Procedure
//================================================================================================

fn main() {
    let config = load_config();
    let db = init_database(&config.database_path).expect("DB初期化失敗");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState { db: Mutex::new(db) })
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                cleanup_on_exit(app_handle);
            }
        });
}

// Win32ネイティブホットキーシステム実装
#[cfg(target_os = "windows")]
fn setup_win32_hotkeys(
    app_handle: AppHandle<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎆 Win32ネイティブホットキー初期化開始...");

    // ホットキースレッドが既に実行中かチェック
    if WIN32_HOTKEY_THREAD_RUNNING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err("ホットキースレッドが既に実行中です".into());
    }

    // バックグラウンドスレッドでWin32ホットキーリスナーを実行
    thread::spawn(move || {
        if let Err(e) = run_win32_hotkey_loop(&app_handle) {
            eprintln!("❌ Win32ホットキーループエラー: {e}");
        }
        WIN32_HOTKEY_THREAD_RUNNING.store(false, Ordering::Relaxed);
    });

    // スレッドの立ち上がりを待機
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("✅ Win32ホットキースレッドが開始されました");
    Ok(())
}

// Win32ホットキーメッセージループ
#[cfg(target_os = "windows")]
fn run_win32_hotkey_loop(
    app_handle: &AppHandle<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔥 Win32ホットキーループ開始");

    unsafe {
        // ホットキーを登録
        let result1 = RegisterHotKey(ptr::null_mut(), HOTKEY_SAVE_ID, MOD_CTRL_SHIFT, VK_S);
        let result2 = RegisterHotKey(ptr::null_mut(), HOTKEY_TOGGLE_ID, MOD_CTRL_SHIFT, VK_A);

        if result1 != 0 {
            println!("✅ Ctrl+Shift+S (Win32) 登録成功");
        } else {
            eprintln!("❌ Ctrl+Shift+S (Win32) 登録失敗");
        }

        if result2 != 0 {
            println!("✅ Ctrl+Shift+A (Win32) 登録成功");
        } else {
            eprintln!("❌ Ctrl+Shift+A (Win32) 登録失敗");
        }

        if result1 == 0 && result2 == 0 {
            return Err("Win32ホットキー登録に失敗しました".into());
        }

        WIN32_HOTKEYS_ACTIVE.store(true, Ordering::Relaxed);
        println!("✅ Win32ホットキーシステムアクティベーション完了");

        // メッセージループ
        let mut msg: MSG = mem::zeroed();
        loop {
            let bret = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
            if bret <= 0 {
                break; // WM_QUIT またはエラー
            }

            if msg.message == WM_HOTKEY {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let hotkey_id = msg.wParam as i32;
                match hotkey_id {
                    HOTKEY_SAVE_ID => {
                        handle_save_hotkey(app_handle);
                    }
                    HOTKEY_TOGGLE_ID => {
                        handle_toggle_hotkey(app_handle);
                    }
                    _ => {}
                }
            }

            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // クリーンアップ
        let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_SAVE_ID);
        let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_TOGGLE_ID);
        WIN32_HOTKEYS_ACTIVE.store(false, Ordering::Relaxed);

        println!("✅ Win32ホットキーループ終了");
    }

    Ok(())
}

// Ctrl+Shift+S ハンドラー
#[cfg(target_os = "windows")]
fn handle_save_hotkey(app_handle: &AppHandle<tauri::Wry>) {
    if !should_execute_hotkey(&LAST_SAVE_HOTKEY) {
        return;
    }

    println!("🔥 Win32: Ctrl+Shift+S アクティベーション");

    let state = app_handle.state::<AppState>();
    match save_active_page(state) {
        Ok(result) => {
            println!("✅ Win32クイック保存完了: {result}");
            REFRESH_NEEDED.store(true, Ordering::Relaxed);
        }
        Err(e) => {
            eprintln!("❌ Win32クイック保存エラー: {e}");
        }
    }
}

// Ctrl+Shift+A ハンドラー
#[cfg(target_os = "windows")]
fn handle_toggle_hotkey(app_handle: &AppHandle<tauri::Wry>) {
    if !should_execute_hotkey(&LAST_TOGGLE_HOTKEY) {
        return;
    }

    println!("🔥 Win32: Ctrl+Shift+A アクティベーション");

    if let Some(window) = app_handle.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                println!("ウィンドウを非表示に");
                let _ = window.hide();
            }
            Ok(false) => {
                println!("ウィンドウを表示");
                let _ = window.show();
                let _ = window.set_focus();
            }
            Err(e) => {
                eprintln!("ウィンドウ状態エラー: {e}");
            }
        }
    }
}

// アプリ終了時のクリーンアップ処理
fn cleanup_on_exit(app_handle: &AppHandle<tauri::Wry>) {
    println!("🧹 アプリ終了 - Win32ホットキークリーンアップを実行中...");

    // Win32ホットキーのクリーンアップ
    #[cfg(target_os = "windows")]
    {
        if WIN32_HOTKEYS_ACTIVE.load(Ordering::Relaxed) {
            unsafe {
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_SAVE_ID);
                let _ = UnregisterHotKey(ptr::null_mut(), HOTKEY_TOGGLE_ID);
                // メッセージループを終了させる
                PostQuitMessage(0);
            }
            WIN32_HOTKEYS_ACTIVE.store(false, Ordering::Relaxed);
            WIN32_HOTKEY_THREAD_RUNNING.store(false, Ordering::Relaxed);
            println!("✅ Win32ホットキークリーンアップ完了");
        }
    }

    // Tauriホットキーのクリーンアップ
    if let Err(e) = app_handle.global_shortcut().unregister_all() {
        eprintln!("⚠️ 終了時のunregister_allエラー: {e}");
    }

    // 全状態をリセット
    HOTKEYS_REGISTERED.store(false, Ordering::Relaxed);

    println!("✅ アプリ終了処理完了");
}

//================================================================================================
// アプリ設定関連ファンクション等 - Functions and Sub procedures for application
//================================================================================================

fn setup_application(app: &mut tauri::App<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    // アプリ起動時セットアップ
    println!("🚀 アプリケーション初期化開始...");

    // システムトレイ作成
    println!("🎛️ システムトレイ作成中...");
    create_system_tray(app.handle())?;

    // グローバルショートカットの設定（エラーが発生してもアプリは継続）
    println!("⌨️ グローバルショートカット設定中...");
    setup_global_shortcuts(app.handle());

    println!("🎉 アプリケーション初期化完了");
    Ok(())
}

fn setup_global_shortcuts(app_handle: &AppHandle<tauri::Wry>) {
    println!("⚙️ グローバルショートカット初期化開始...");

    // Windowsは生のWin32 APIを優先（Tauriプラグインより挙動が安定するため）
    #[cfg(target_os = "windows")]
    {
        match setup_win32_hotkeys(app_handle.clone()) {
            Ok(()) => {
                println!("✅ Win32ネイティブホットキーシステム初期化成功");
                WIN32_HOTKEYS_ACTIVE.store(true, Ordering::Relaxed);
                HOTKEYS_REGISTERED.store(true, Ordering::Relaxed);
                return;
            }
            Err(e) => {
                println!("⚠️ Win32ホットキー初期化失敗: {e} - Tauriグローバルショートカットにフォールバック");
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("ℹ️ Windows以外のプラットフォーム: Tauriグローバルショートカットで登録");
    }

    let mut success_count = 0;
    #[allow(clippy::collection_is_never_read)]
    let mut error_messages = Vec::new();

    // Ctrl+Shift+S: クイック保存
    match register_hotkey(app_handle, "ctrl+shift+s", "Ctrl+Shift+S", |app_handle| {
        if !should_execute_hotkey(&LAST_SAVE_HOTKEY) {
            return;
        }

        println!("🔥 Ctrl+Shift+S が押されました - クイック保存を実行");
        let state = app_handle.state::<AppState>();
        match save_active_page(state) {
            Ok(result) => {
                println!("✅ クイック保存完了: {result}");
                REFRESH_NEEDED.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("❌ クイック保存エラー: {e}");
            }
        }
    }) {
        Ok(()) => {
            success_count += 1;
            println!("✅ Ctrl+Shift+S セットアップ成功");
        }
        Err(e) => {
            error_messages.push(format!("Ctrl+Shift+S: {e}"));
            println!("⚠️ Ctrl+Shift+S セットアップ失敗: {e}");
        }
    }

    // Ctrl+Shift+A: ウィンドウ表示切替
    match register_hotkey(app_handle, "ctrl+shift+a", "Ctrl+Shift+A", |app_handle| {
        if !should_execute_hotkey(&LAST_TOGGLE_HOTKEY) {
            return;
        }

        println!("🔥 Ctrl+Shift+A が押されました - ウィンドウ表示切替");
        if let Some(window) = app_handle.get_webview_window("main") {
            match window.is_visible() {
                Ok(true) => {
                    println!("ウィンドウを非表示に");
                    let _ = window.hide();
                }
                Ok(false) => {
                    println!("ウィンドウを表示");
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                Err(e) => {
                    eprintln!("ウィンドウ状態取得エラー: {e}");
                }
            }
        }
    }) {
        Ok(()) => {
            success_count += 1;
            println!("✅ Ctrl+Shift+A セットアップ成功");
        }
        Err(e) => {
            error_messages.push(format!("Ctrl+Shift+A: {e}"));
            println!("⚠️ Ctrl+Shift+A セットアップ失敗: {e}");
        }
    }

    // 結果の評価（フォールバック戦略の場合）
    if success_count == 2 {
        println!("🎉 全てのグローバルショートカット設定完了 ({success_count}/2)");
        HOTKEYS_REGISTERED.store(true, Ordering::Relaxed);
    } else if success_count > 0 {
        println!("⚠️ 一部のグローバルショートカット設定完了 ({success_count}/2)");
        println!("   📝 設定できなかったホットキーは、システムトレイから操作してください");
        HOTKEYS_REGISTERED.store(true, Ordering::Relaxed);
    } else {
        println!("❌ グローバルショートカット設定失敗");
        println!("   💡 システムトレイから全ての操作が可能です");
        println!("   ℹ️ ホットキーの代わりにシステムトレイを使用してください");
        // アプリは継続（エラーではなく機能制限）
    }

    println!("   📝 ホットキーが使用できない場合は、システムトレイから操作してください");
}

// Tauriプラグイン経由でのグローバルショートカット登録（Windows以外のメイン経路、Windowsのフォールバック経路）
fn register_hotkey<F>(
    app_handle: &AppHandle<tauri::Wry>,
    shortcut_str: &str,
    display_name: &str,
    callback: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(&AppHandle<tauri::Wry>) + Send + Sync + 'static + Clone,
{
    println!("🔧 {display_name} をTauriグローバルショートカットとして登録中 ({shortcut_str})...");
    app_handle
        .global_shortcut()
        .on_shortcut(shortcut_str, move |app_handle, _shortcut, event| {
            // キー押下・離上の両方でイベントが飛んでくるため、押下時のみ実行する
            if event.state == ShortcutState::Pressed {
                callback(app_handle);
            }
        })
        .map_err(|e| e.to_string().into())
}

// デバウンス機能付きヘルパー関数
fn should_execute_hotkey(last_timestamp: &AtomicU64) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let last = last_timestamp.load(Ordering::Relaxed);

    if now.saturating_sub(last) < DEBOUNCE_MS {
        println!("⏱️ ホットキーデバウンス中 - 実行をスキップ");
        return false;
    }

    last_timestamp.store(now, Ordering::Relaxed);
    true
}

fn handle_window_event(window: &tauri::Window<tauri::Wry>, event: &tauri::WindowEvent) {
    // ウィンドウまわり制御

    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        // ウィンドウを閉じる代わりに隠す
        window.hide().unwrap();
        api.prevent_close();
    }
}

// システムトレイの設定
fn create_system_tray(
    app_handle: &AppHandle<tauri::Wry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let menu = MenuBuilder::new(app_handle)
        .item(&MenuItem::with_id(
            app_handle,
            "show",
            "表示/非表示切替",
            true,
            None::<&str>,
        )?)
        .separator()
        .item(&MenuItem::with_id(
            app_handle,
            "save_page",
            "現在のページを保存",
            true,
            None::<&str>,
        )?)
        .separator()
        .item(&MenuItem::with_id(
            app_handle,
            "quit",
            "終了",
            true,
            None::<&str>,
        )?)
        .build()?;

    let app_handle_clone = app_handle.clone();
    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .icon(app_handle.default_window_icon().unwrap().clone())
        .on_menu_event(move |_app, event| {
            handle_system_tray_menu_event(&app_handle_clone, &event);
        })
        .on_tray_icon_event(move |tray, event| {
            handle_system_tray_click_event(tray, &event);
        })
        .build(app_handle)?;

    Ok(())
}

// システムトレイメニューイベントの処理
fn handle_system_tray_menu_event(app: &AppHandle<tauri::Wry>, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                match window.is_visible() {
                    Ok(true) => {
                        let _ = window.hide();
                        println!("システムトレイ: ウィンドウを非表示");
                    }
                    Ok(false) => {
                        let _ = window.show();
                        let _ = window.set_focus();
                        println!("システムトレイ: ウィンドウを表示");
                    }
                    Err(_) => {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        }
        "save_page" => {
            // アクティブページ保存をバックグラウンドで実行
            let app_state = app.state::<AppState>();
            match save_active_page(app_state) {
                Ok(result) => println!("トレイからページを保存しました: {result}"),
                Err(e) => eprintln!("トレイからの保存エラー: {e}"),
            }
        }
        "quit" => {
            println!("🧹 システムトレイから終了 - ホットキーを解除中...");
            cleanup_on_exit(app);
            std::process::exit(0);
        }
        _ => {}
    }
}

// システムトレイクリックイベントの処理
fn handle_system_tray_click_event(tray: &tauri::tray::TrayIcon<tauri::Wry>, event: &TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        // 左クリックでウィンドウの表示/非表示切り替え
        println!("トレイアイコンが左クリックされました - ウィンドウ切り替え");
        let app_handle = tray.app_handle();
        if let Some(window) = app_handle.get_webview_window("main") {
            match window.is_visible() {
                Ok(true) => {
                    let _ = window.hide();
                    println!("トレイクリック: ウィンドウを非表示");
                }
                Ok(false) => {
                    let _ = window.show();
                    let _ = window.set_focus();
                    println!("トレイクリック: ウィンドウを表示");
                }
                Err(_) => {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
    }
}

//================================================================================================
// Tauri用コマンド設定 - Commands for Tauri
//================================================================================================

// リフレッシュが必要かチェックするコマンド
#[tauri::command]
fn check_refresh_needed() -> bool {
    // 更新時に発火
    let needed = REFRESH_NEEDED.load(Ordering::Relaxed);
    if needed {
        REFRESH_NEEDED.store(false, Ordering::Relaxed);
        println!("✅ リフレッシュフラグをクリアしました");
    }
    needed
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::significant_drop_tightening)]
fn get_articles(
    state: State<AppState>,
    filters: Option<SearchFilters>,
) -> Result<Vec<ArticleWithDetails>, String> {
    // 記事検索
    let db = state.db.lock().map_err(|e| e.to_string())?;

    let mut query = "SELECT 
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
     
    "
    .to_string();

    let mut conditions = Vec::new();
    let mut params: Vec<String> = Vec::new();

    if let Some(filters) = filters {
        // フィルター処理：サイト
        if let Some(site) = filters.site {
            conditions.push("s.name LIKE ?".to_string());
            params.push(format!("%{site}%"));
        }
        // フィルター処理：タグ
        if let Some(tag_query) = filters.tag_query {
            let search_tags: Vec<&str> = tag_query.split(',').map(str::trim).collect();
            for tag in search_tags {
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
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let articles = stmt
        .query_map(&param_refs[..], |row| {
            let tags_str: Option<String> = row.get(4)?;
            let tags = tags_str.map_or_else(Vec::new, |tags_str| {
                tags_str
                    .split(',')
                    .map(|tag| tag.trim().to_string())
                    .collect()
            });
            Ok(ArticleWithDetails {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                site_name: Some(row.get(3)?),
                tags,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for article in articles {
        result.push(article.map_err(|e| e.to_string())?);
    }

    Ok(result)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::option_if_let_else)]
fn save_article(state: State<AppState>, request: SaveArticleRequest) -> Result<String, String> {
    println!("記事保存開始: {}", request.url);

    let db = state.db.lock().map_err(|e| e.to_string())?;

    let normalized_url = normalize_url(&request.url);
    let parsed_url = Url::parse(&normalized_url).map_err(|e| e.to_string())?;
    let site_name = parsed_url.host_str().unwrap_or("").replace("www.", "");

    // ph.1 サイトID確定
    let site_id = get_or_create_site(&db, &site_name)?;

    // ph.2 既存記事をチェック
    let existing_article = db
        .prepare("SELECT id FROM articles WHERE url = ?")
        .and_then(|mut stmt| {
            stmt.query_row([&normalized_url], |row| row.get::<_, i64>(0))
                .optional()
        })
        .map_err(|e| e.to_string())?;

    let (article_id, result_status) = if let Some(existing_id) = existing_article {
        // 既存記事を更新
        println!("既存記事を更新: {} (ID: {})", request.title, existing_id);
        db.execute(
            "UPDATE articles SET title = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            params![request.title, existing_id],
        )
        .map_err(|e| e.to_string())?;

        // 既存のタグ関連を削除
        db.execute(
            "DELETE FROM article_tags WHERE article_id = ?",
            params![existing_id],
        )
        .map_err(|e| e.to_string())?;

        (existing_id, "updated".to_string())
    } else {
        // 新規記事を作成
        println!("記事保存開始：{}", request.url);
        db.execute(
            "INSERT INTO articles (url, title, site_id, created_at, updated_at) 
             VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            params![normalized_url, request.title, site_id],
        )
        .map_err(|e| e.to_string())?;

        let new_id = db.last_insert_rowid();
        println!("記事作成完了: {} (ID: {})", request.title, new_id);
        (new_id, "created".to_string())
    };

    // ph.3 タグの処理
    if let Some(tags_str) = request.tags {
        let tag_names: Vec<&str> = tags_str.split(',').map(str::trim).collect();
        for tag_name in tag_names {
            println!("処理中のタグ: '{tag_name}'");
            if !tag_name.is_empty() {
                // 1. タグID取得/作成
                let tag_id = get_or_create_tag(&db, tag_name)?;

                // 2. 記事-タグ関連を作成
                println!("article_tags への INSERT: article_id={article_id}, tag_id={tag_id}");

                match db.execute(
                    "INSERT INTO article_tags (article_id, tag_id) VALUES (?, ?)",
                    params![article_id, tag_id],
                ) {
                    Ok(rows) => println!("article_tags INSERT 成功: {rows} rows"),
                    Err(e) => println!("article_tags INSERT エラー: {e}"),
                }
            }
        }
    } else {
        println!("タグなし（None）");
    }

    println!("記事保存完了: {result_status}");
    Ok(result_status)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::significant_drop_tightening)]
fn update_article(state: State<AppState>, request: SaveArticleRequest) -> Result<(), String> {
    println!("記事編集開始: {}", request.url);
    let db = state.db.lock().map_err(|e| e.to_string())?;

    // ph.1 更新対象の記事ID特定
    let article_id = get_article_id_by_url(&db, &request.url)?;

    // ph.2 記事の基本情報を更新
    db.execute(
        "UPDATE articles SET title = ?, url = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        params![request.title, request.url, article_id],
    )
    .map_err(|e| e.to_string())?;

    println!("記事基本情報更新完了");

    // ph.3 既存のタグ-記事リレーションを削除
    db.execute(
        "DELETE FROM article_tags WHERE article_id = ?",
        params![article_id],
    )
    .map_err(|e| e.to_string())?;
    println!("既存タグ関連削除完了");

    // ph.4 タグ-記事リレーションを改めて登録
    if let Some(tags_str) = request.tags {
        let tag_names: Vec<&str> = tags_str.split(',').map(str::trim).collect();
        for tag_name in tag_names {
            if !tag_name.is_empty() {
                let tag_id = get_or_create_tag(&db, tag_name)?;
                db.execute(
                    "INSERT INTO article_tags (article_id, tag_id) VALUES (?, ?)",
                    params![article_id, tag_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        println!("新しいタグ登録完了");
    }
    println!("記事編集完了");
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::significant_drop_tightening)]
fn delete_article(state: State<AppState>, url: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    db.execute("DELETE FROM articles WHERE url = ?", params![url])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn open_url(url: String) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn save_active_page(state: State<AppState>) -> Result<String, String> {
    println!("自動保存開始...");

    let browser_info = match get_active_browser_info() {
        Ok(info) => {
            println!("✅ browser-infoライブラリでブラウザ情報取得成功");
            info
        }
        Err(e) => {
            println!("❌ browser-infoライブラリでの取得失敗: {e}");
            return Err(format!("ブラウザ情報取得失敗: {e}"));
        }
    };

    // タグ自動生成
    let auto_tags = auto_tagging(browser_info.url.clone());
    println!("生成されたタグ: {auto_tags}");

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
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::significant_drop_tightening)]
fn get_popular_tags(state: State<AppState>, limit: Option<usize>) -> Result<Vec<TagCount>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(20);

    let mut stmt = db
        .prepare(
            "SELECT
           TRIM(t.name) as tag_name
         , COUNT(*) as count
        FROM tags t
        JOIN article_tags at
          ON t.id = at.tag_id
        WHERE t.name != 'auto-saved' --暫定
        GROUP BY TRIM(t.name) 
        ORDER BY count DESC, t.name ASC
        LIMIT ?",
        )
        .map_err(|e| e.to_string())?;

    let tag_counts = stmt
        .query_map([limit], |row| {
            Ok(TagCount {
                tag: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;

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
            format!(
                "{}{}",
                parsed_url.origin().ascii_serialization(),
                parsed_url.path()
            )
        }
        Err(_) => url.to_string(),
    }
}

// 登録サイトIDの特定
fn get_or_create_site(db: &Connection, site_name: &str) -> Result<i64, String> {
    // 登録済みサイトの検索（重複確認）
    let mut stmt = db
        .prepare("SELECT id FROM sites WHERE name = ?")
        .map_err(|e| e.to_string())?;

    let site_id_opt = stmt
        .query_row([site_name], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(site_id) = site_id_opt {
        println!("既存サイト使用: {site_name} (ID: {site_id})");
        Ok(site_id)
    } else {
        // 新しいサイトを作成（INSERT）
        db.execute("INSERT INTO sites (name) VALUES (?)", params![site_name])
            .map_err(|e| e.to_string())?;

        // 作成したサイトのIDを取得
        let site_id = db.last_insert_rowid();
        println!("新規サイト作成: {site_name} (ID: {site_id})");
        Ok(site_id)
    }
}

// 登録に使うタグの特定
fn get_or_create_tag(db: &Connection, tag_name: &str) -> Result<i64, String> {
    let mut stmt = db
        .prepare("SELECT id FROM tags WHERE name = ?")
        .map_err(|e| e.to_string())?;

    let tag_id_opt = stmt
        .query_row([tag_name], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some(tag_id) = tag_id_opt {
        println!("既存タグ使用: {tag_name} (ID: {tag_id})");
        Ok(tag_id)
    } else {
        // 新しいタグを作成
        db.execute("INSERT INTO tags (name) VALUES (?)", [tag_name])
            .map_err(|e| e.to_string())?;

        let tag_id = db.last_insert_rowid();
        println!("新規タグ作成: {tag_name} (ID: {tag_id})");
        Ok(tag_id)
    }
}

// 記事IDをurlから求める
fn get_article_id_by_url(db: &Connection, url: &str) -> Result<i64, String> {
    let mut stmt = db
        .prepare("SELECT id FROM articles WHERE url = ?")
        .map_err(|e| e.to_string())?;

    stmt.query_row([url], |row| row.get(0))
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

fn load_config() -> Config {
    let config_path = PathBuf::from("config.json");

    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => match serde_json::from_str::<Config>(&content) {
                Ok(config) => {
                    println!(
                        "✅ 設定ファイル読み込み成功: database_path = {}",
                        config.database_path
                    );
                    return config;
                }
                Err(e) => {
                    eprintln!("⚠️ 設定ファイルのパースエラー: {e} - デフォルト設定を使用");
                }
            },
            Err(e) => {
                eprintln!("⚠️ 設定ファイル読み込みエラー: {e} - デフォルト設定を使用");
            }
        }
    } else {
        println!("ℹ️ 設定ファイルが見つかりません - デフォルト設定を使用");
    }

    // デフォルト設定
    Config {
        database_path: "atode.db".to_string(),
    }
}

fn init_database(db_path: &str) -> Result<Connection> {
    println!("📂 データベースパス: {db_path}");
    let conn = Connection::open(db_path)?;

    conn.execute("PRAGMA foreign_keys = ON;", [])?;

    let ddl_files = [
        include_str!("ddl/001_create_sites.sql"),
        include_str!("ddl/002_create_articles.sql"),
        include_str!("ddl/003_create_tags.sql"),
        include_str!("ddl/004_create_article_tags.sql"),
    ];

    for ddl in &ddl_files {
        conn.execute(ddl, [])?;
    }

    Ok(conn)
}

// 自動タグ付け
#[allow(clippy::needless_pass_by_value)]
fn auto_tagging(url: String) -> String {
    let mut tags: Vec<String> = Vec::with_capacity(3);

    // URLクレートでサイト名を抽出
    if let Ok(parsed_url) = Url::parse(&url) {
        if let Some(host) = parsed_url.host_str() {
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
    if let Some(captures) = PREFIX_RE.find(&input) {
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
        "claude" | "chatgpt" | "openai" | "anthropic" | "gemini" => {
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
mod tests {
    use super::*;

    #[test]
    fn test_url_normalization() {
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
    fn test_auto_tagging() {
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
