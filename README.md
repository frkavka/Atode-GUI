# Atode - 後で読む記事管理ツール v0.1

Webページを素早く保存して、後で読むためのデスクトップアプリケーションです。

## 機能

- 🔗 **クイック保存**: `Ctrl+Shift+S` でアクティブなブラウザページを瞬時に保存（※現在Windowsのみ対応）
- 🏷️ **タグ管理**: 記事をタグで分類・検索          → 記事についているタグをクリックするとそれですぐに絞り込めます
- 🌐 **サイト検索**: 特定のサイトの記事を絞り込み   → 記事についているサイト名をクリックするとそれですぐに絞り込めます
- 📱 **システムトレイ**: バックグラウンドで常駐
- 💾 **ローカル保存**: SQLiteでデータを安全に保存
![image](https://github.com/user-attachments/assets/1a52fce9-402a-4df4-a220-b1c520c22447)


## セットアップ

### 必要な環境
- Node.js
- Rust
- Windows (現在Windowsのみ対応)

### インストール
```bash
git clone https://github.com/frkavka/Atode-GUI.git
cd Atode-GUI
npm install
npm run dev
```

## ショートカットキー
- `Ctrl+Shift+S`: 現在のブラウザページを保存
- `Ctrl+Shift+A`: アプリの表示/非表示切り替え

## 技術スタック
- **フロントエンド**: HTML/CSS/JavaScript
- **バックエンド**: Rust (Tauri)
- **データベース**: SQLite
- **ビルド**: Tauri v1.6.3
