# Atode - 後で読む記事管理ツール v1.1
[English](README.md) | [日本語](README.ja.md)

### `Ctrl+Shift+S`で0.5秒でWebページを保存
![Atodemo1](https://github.com/user-attachments/assets/6a1d3ae5-16a1-482c-a530-8a5587ddc242)

![Atodemo2](https://github.com/user-attachments/assets/cac7a776-3e42-433d-8b81-41d1c5f14174)

**こんなときありませんか？**
- `面白そうな記事だけど、今は時間がないな…`
- `実装の参考に取っておきたいな…`
- `技術資料をWebで簡単にアクセスできるようにしたい`

**そんなときこのアプリが解決します**
これは`後で読むWebページを保存・管理する`デスクトップアプリケーションで、TauriとRustで構築されているため`高速で軽量`です。

<a href="https://github.com/frkavka/Atode-GUI/releases">📥 ダウンロード</a><br>
<a href="https://github.com/frkavka/Atode-GUI/issues"> 🐛 バグ報告</a><br>
<a href="https://github.com/frkavka/Atode-GUI/discussions"> 💡 要望・提案</a>



## 機能

- ⚡ **超高速保存**: `Ctrl+Shift+S`でページを瞬時に保存 - ブラウジングを邪魔しません
- 🏷️ **スマートタグ機能**: `javascript`、`チュートリアル`、`研究`などのタグで記事を整理
- 🌐 **サイト絞り込み**: 特定サイト（例：「github」「stackoverflow」）の記事をすべて検索
- 🤫 **静かな動作**: 煩わしい通知なし - バックグラウンドで静かに動作
- 📱 **システムトレイ**: `Ctrl+Shift+A`で常にアクセス可能、邪魔にならない
- 💾 **100%プライベート**: すべてのデータをSQLiteでローカル保存 - クラウド無し、追跡無し
- 🚀 **Rust性能**: RAM使用量約～20MBで軽量です
- 🎯 **クロスブラウザ対応**: Chrome、Firefox、Edge、Brave、Opera で動作

## クイックスタート
### Windowsユーザー

1. 最新の`.msi`ファイルを <a href="https://github.com/frkavka/Atode-GUI/releases">リリース</a> からダウンロード<br>
2. Atodeをインストールして実行
3. 完了！任意の記事を見ながら`Ctrl+Shift+S`を押すだけ

### 開発者
```bash
git clone https://github.com/frkavka/Atode-GUI.git
cd Atode-GUI
npm install
npm run dev:windows

必要環境: Node.js, Rust, Windows（現在）
```

## 📖 使用方法
### 基本ワークフロー

1. **ブラウジング**: 任意のWebサイトで興味深い記事を見つける
2. **保存**: Ctrl+Shift+Sを押す（どのブラウザでも動作）
3. **整理**: Atodeを開いて、タグを追加、必要に応じてタイトルを編集
4. **検索**: 後でタグやサイト名で検索
5. **読む**: 記事タイトルをクリックしてブラウザで開く

## キーボードショートカット
### ショートカットキー
- `Ctrl+Shift+S`: 現在のブラウザページを保存
- `Ctrl+Shift+A`: アプリウィンドウの表示/非表示

## 技術スタック
- フロントエンド: HTML/CSS/JavaScript
- バックエンド: Rust (Tauri v2.0)
- データベース: SQLite
- プラットフォーム: 現在Windows対応

## タグ使用例
- プロジェクト別: `project-a`、`project-b`
- 技術別: `javascript`、`python`
- 興味別: `ゲーム情報`、`2025年夏アニメ`

## 今後のアイデア
```
注：実際に取り組むことは約束できません
```
- [ ] macOS対応
- [ ] Linux対応
- [ ] 多言語サポート
- [ ] 特定サイトでのクエリパラメータ保持（現在はYouTubeのみ可能）
- [ ] エクスポート/インポート機能
- [ ] メモ機能
- [ ] AI活用機能

## 🤝 コントリビューション

- 🐛 バグ報告: Issueを作成
- 💡 機能提案: Discussionを開始
- 🔧 コード提出: Fork、コーディング、プルリクエスト作成

## ライセンス
MIT License

---
**⭐ 便利だと思ったらスターをお願いします！**<br>

