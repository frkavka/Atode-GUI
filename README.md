# Atode - Read Later Article Manager v0.1

[English](README.md) | [日本語](README.ja.md)

**When you think:**
- `I found an interesting web page, but I can't read it right now.`
- `I want to make technical references on the web easily accessible.`

**Then this app helps you.**
This is a desktop application for `saving and managing web pages to read later`, built with Tauri and Rust; therefore, it's `quick and light`.

## Features

- 🔗 **Quick Save**: The hotkey `Ctrl+Shift+S` instantly saves a web page you're viewing (Windows only for now)
- 🏷️ **Tag Management**: You can set tags on each saved article and search them by tags
- 🌐 **Site filtering**: You can search saved articles by their sites such as 'google'
- 📱 **System Tray**: Runs in background, so always accesible via `Ctrl+Shift+A`
- 💾 **Local Storage**: Data saved securely with SQLite
- ⚡ **Extremely Lightweight**: Only ~20MB RAM usage (vs 300MB+ for typical Electron apps)
- 🚀 **Fast Performance**: Built with Tauri/Rust for native speed

## Installation (for developping ver)
### Requirements
```bash
Node.js
Rust
Windows (macOS/Linux support planned)
```
### Setup
```
git clone https://github.com/frkavka/Atode-GUI.git
cd Atode-GUI
npm install
npm run dev
```

## Usage
### Keyboard Shortcuts
- `Ctrl+Shift+S`: Save current browser page
- `Ctrl+Shift+A`: Show/hide app window

### Quick Workflow(for Win)

1. Browse the web normally
2. Found something interesting? Press Ctrl+Shift+S
3. Article is automatically saved with current page title and URL
4. Open Atode later to browse, search, and read your saved articles

## Tech Stack
- Frontend: HTML/CSS/JavaScript
- Backend: Rust (Tauri v1.6.3)
- Database: SQLite
- Platform: currently for Windows

## Roadmap(draft)
- [ ] macOS and Linux support
- [ ] preserve query parameters for specific sites(currently, possible only for youtube)
- [ ] Export/import functionality
- [ ] add note section on each article
- [ ] something, utilizing AI 

## Contributing
Contributions are welcome! Please feel free to submit issues, request a function, pull request.

## License
MIT License
