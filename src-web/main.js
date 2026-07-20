// Tauri 2.0 API
const invoke = window.__TAURI_INTERNALS__.invoke;

class AtodeApp {
    constructor() {
        this.articles = [];
        this.editingUrl = null;
        this.popularTags = [];
        this.popularSites = [];
        this._confirmResolve = null;
        this.init();
    }

    async init() {
        this.loadTheme();
        await this.loadArticles();
        await this.loadPopularTags();
        this.setupEventListeners();
        this.setupPeriodicRefresh();
    }

    loadTheme() {
        const savedTheme = localStorage.getItem('theme');
        const isDark = savedTheme === 'dark' || (!savedTheme && window.matchMedia('(prefers-color-scheme: dark)').matches);

        if (isDark) {
            document.body.classList.add('dark-mode');
            this.updateThemeIcon(true);
        } else {
            this.updateThemeIcon(false);
        }
    }

    toggleTheme() {
        const isDark = document.body.classList.toggle('dark-mode');
        localStorage.setItem('theme', isDark ? 'dark' : 'light');
        this.updateThemeIcon(isDark);
    }

    updateThemeIcon(isDark) {
        const icon = document.getElementById('themeIcon');
        if (icon) {
            icon.textContent = isDark ? '☀️' : '🌙';
        }
    }

    setupEventListeners() {
        // Enter キーでの検索
        const tagSearch = document.getElementById('tagSearch');
        const siteSearch = document.getElementById('siteSearch');
        
        [tagSearch, siteSearch].forEach(input => {
            if (input) {
                input.addEventListener('keypress', (e) => {
                    if (e.key === 'Enter') {
                        this.searchArticles();
                    }
                });
            }
        });

        // モーダルの外側クリックで閉じる
        const modal = document.getElementById('articleModal');
        if (modal) {
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    this.closeModal();
                }
            });
        }

        // 確認モーダルのボタン・外側クリック
        const confirmModal = document.getElementById('confirmModal');
        document.getElementById('confirmOkBtn')?.addEventListener('click', () => this.resolveConfirm(true));
        document.getElementById('confirmCancelBtn')?.addEventListener('click', () => this.resolveConfirm(false));
        if (confirmModal) {
            confirmModal.addEventListener('click', (e) => {
                if (e.target === confirmModal) {
                    this.resolveConfirm(false);
                }
            });
        }

        // Escape キーでモーダルを閉じる
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                this.resolveConfirm(false);
                this.closeModal();
            }
        });
    }

    // ネイティブconfirm()はTauriのmacOS向けWebViewで未実装のため、
    // 自前モーダルで代替する
    confirmDialog(message) {
        const modal = document.getElementById('confirmModal');
        const messageEl = document.getElementById('confirmMessage');
        if (!modal || !messageEl) return Promise.resolve(false);

        messageEl.textContent = message;
        modal.style.display = 'block';

        return new Promise((resolve) => {
            this._confirmResolve = resolve;
        });
    }

    resolveConfirm(result) {
        const modal = document.getElementById('confirmModal');
        if (modal) modal.style.display = 'none';
        if (this._confirmResolve) {
            this._confirmResolve(result);
            this._confirmResolve = null;
        }
    }

    setupPeriodicRefresh() {
        // 500ms毎にリフレッシュが必要かチェック
        setInterval(async () => {
            try {
                const needRefresh = await invoke('check_refresh_needed');
                if (needRefresh) {
                    console.log('🔄 リフレッシュが必要です。記事リストを更新します。');
                    await this.loadArticles();
                    await this.loadPopularTags();
                }
            } catch (error) {
                console.error('リフレッシュチェックエラー:', error);
            }
        }, 500);
    }

    async loadPopularTags() {
        try {
            this.popularTags = await invoke('get_popular_tags', { limit: 15 });
            console.log('人気タグ読み込み完了:', this.popularTags.length, '件');
        } catch (error) {
            console.error('人気タグ読み込みエラー:', error);
            this.popularTags = [];
        }
    }

    async loadArticles() {
        try {
            // 検索条件をクリア
            const tagSearch = document.getElementById('tagSearch');
            const siteSearch = document.getElementById('siteSearch');
            
            if (tagSearch) {
                tagSearch.value = '';
                tagSearch.placeholder = '🏷️ タグで検索 (カンマ区切り入力)'; // プレースホルダーリセット
            }
            if (siteSearch) {
                siteSearch.value = '';
                siteSearch.placeholder = '🌐 サイトで検索(例：google)';
            }
            
            this.articles = await invoke('get_articles');
            this.renderArticles();
            console.log(`📚 ${this.articles.length}件の記事を読み込みました`);
        } catch (error) {
            console.error('記事の読み込みエラー:', error);
            this.showError('記事の読み込みに失敗しました');
        }
    }

    async searchArticles() {
        const tagQuery = document.getElementById('tagSearch')?.value.trim();
        const site = document.getElementById('siteSearch')?.value.trim();

        const filters = {};
        if (tagQuery) {
            // カンマ+スペースをカンマに統一して小文字化
            const normalizedTags = normalizeTagString(tagQuery).toLowerCase();
            filters.tag_query = normalizedTags;
        }
        
        if (site) filters.site = site;

        try {
            this.articles = await invoke('get_articles', { filters });
            this.renderArticles();
            console.log(`🔍 検索結果: ${this.articles.length}件`);
        } catch (error) {
            console.error('検索エラー:', error);
            this.showError('検索に失敗しました');
        }
    }

    renderArticles() {
        const container = document.getElementById('articleList');
        if (!container) return;

        if (this.articles.length === 0) {
            container.innerHTML = `
                <div class="empty-state">
                    <h3>📚 記事がありません</h3>
                    <p>右上の「記事を追加」ボタンから記事を追加するか、<br>
                    Ctrl+Shift+S でアクティブなブラウザのページを保存してください。</p>
                </div>
            `;
            return;
        }

        container.innerHTML = this.articles.map(article => this.renderArticle(article)).join('');
    }

    renderArticle(article) {
        //const tags = article.tags ? article.tags.split(',').map(tag => tag.trim()) : [];
        const tags = article.tags || []; // 既に配列なのでそのまま使用
        const tagsHtml = tags.map(tag => 
            `<span class="tag clickable-tag" onclick="app.handleTagClick('${this.escapeHtml(tag)}', 'search')" title="このタグで検索">
                ${this.escapeHtml(tag)}
            </span>`
        ).join('');
        
        const updatedDate = new Date(article.updated_at).toLocaleDateString('ja-JP', {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        });

        return `
            <div class="article-item">
                <div class="article-title" onclick="app.openArticle('${this.escapeHtml(article.url)}')">
                    ${this.escapeHtml(article.title)}
                </div>
                <div class="article-meta">
                    <span class="site-link" onclick="app.addToSiteSearch('${this.escapeHtml(article.site_name)}')" title="このサイトで検索">
                        ${this.escapeHtml(article.site_name)}
                    </span> • ${updatedDate}
                </div>
                <div class="article-tags">${tagsHtml}</div>
                <div class="article-actions">
                    <button class="btn-small" onclick="app.editArticle('${this.escapeHtml(article.url)}')">
                        編集
                    </button>
                    <button class="btn-small btn-danger" onclick="app.deleteArticle('${this.escapeHtml(article.url)}')">
                        削除
                    </button>
                </div>
            </div>
        `;
    }

    async openArticle(url) {
        try {
            await invoke('open_url', { url });
        } catch (error) {
            console.error('URL オープンエラー:', error);
            this.showError('URLを開けませんでした');
        }
    }

    showAddModal() {
        this.editingUrl = null;
        this.resetForm();
        
        const modal = document.getElementById('articleModal');
        const modalTitle = document.getElementById('modalTitle');
        
        if (modalTitle) modalTitle.textContent = '記事を追加';
        if (modal) modal.style.display = 'block';
        
        this.renderTagSuggestions();
        
        const urlInput = document.getElementById('urlInput');
        urlInput?.focus();
    }

    async editArticle(url) {
        const article = this.articles.find(a => a.url === url);
        if (!article) return;

        this.editingUrl = url;
        
        const urlInput = document.getElementById('urlInput');
        const titleInput = document.getElementById('titleInput');
        const tagsInput = document.getElementById('tagsInput');
        const modalTitle = document.getElementById('modalTitle');
        const modal = document.getElementById('articleModal');

        if (urlInput) urlInput.value = article.url;
        if (titleInput) titleInput.value = article.title;
        if (tagsInput) tagsInput.value = article.tags || '';
        if (modalTitle) modalTitle.textContent = '記事を編集';
        if (modal) modal.style.display = 'block';

        this.renderTagSuggestions();
        titleInput?.focus();
    }

    async deleteArticle(url) {
        if (!(await this.confirmDialog('この記事を削除しますか？'))) return;

        try {
            await invoke('delete_article', { url });
            await this.loadArticles();
            await this.loadPopularTags();
            this.showSuccess('記事を削除しました');
        } catch (error) {
            console.error('削除エラー:', error);
            this.showError('記事の削除に失敗しました');
        }
    }

    async handleSubmit() {
        const urlInput = document.getElementById('urlInput');
        const titleInput = document.getElementById('titleInput');
        const tagsInput = document.getElementById('tagsInput');

        if (!urlInput?.value.trim() || !titleInput?.value.trim()) {
            this.showError('URLとタイトルは必須です');
            return;
        }

        const request = {
            url: urlInput.value.trim(),
            title: titleInput.value.trim(),
            tags: tagsInput.value.trim() || undefined
        };

        try {
            if (this.editingUrl) {
                await invoke('update_article', {request });
                this.showSuccess('記事を更新しました');
            } else {
                // save_articleも同様に修正
                const result = await invoke('save_article', {request });
                this.showSuccess(result === 'created' ? '記事を追加しました' : '記事を更新しました');
            }
            
            this.closeModal();
            await this.loadArticles();
            await this.loadPopularTags(); // タグ統計も更新
        } catch (error) {
            console.error('保存エラー:', error);
            this.showError('記事の保存に失敗しました');
        }
    }

    closeModal() {
        const modal = document.getElementById('articleModal');
        if (modal) modal.style.display = 'none';
        this.resetForm();
        this.editingUrl = null;
    }

    resetForm() {
        const form = document.getElementById('articleForm');
        form?.reset();
        
        const existingSuggestions = document.querySelector('.tag-suggestions');
        if (existingSuggestions) {
            existingSuggestions.remove();
        }
    }

    handleTagClick(tagName, action) {
        switch(action) {
            case 'search':
                this.addToSearchBox(tagName);
                break;
            case 'input':
                this.addToInputField(tagName);
                break;
        }
    }

    addToSearchBox(tagName) {
        const tagSearch = document.getElementById('tagSearch');
        if (!tagSearch) return;
        
        // 余計なスペース等除去 + 大文字小文字の区別なし
        const cleanTagName = normalizeTagString(tagName).toLowerCase();
        const currentValue = tagSearch.value.trim();
        
        if (currentValue) {
            const cleanCurrentValue = normalizeTagString(currentValue).toLowerCase();
            const tags = currentValue.split(',').map(t => t.trim());
            if (!tags.includes(cleanTagName)) {
                tagSearch.value = tags.concat(cleanTagName).join(','); // カンマ区切りで統一
            }
        } else {
            tagSearch.value = cleanTagName;
    }

        this.searchArticles();
    }

    addToInputField(tagName) {
        const tagsInput = document.getElementById('tagsInput');
        if (!tagsInput) return;

        const currentValue = tagsInput.value.trim();
        if (currentValue) {
            const tags = currentValue.split(',').map(t => t.trim());
            if (!tags.includes(tagName)) {
                tagsInput.value = tags.concat(tagName).join(', ');
            }
        } else {
            tagsInput.value = tagName;
        }
    }

    addToSiteSearch(siteName) {
        const siteSearch = document.getElementById('siteSearch');
        if (!siteSearch) return;

        siteSearch.value = siteName;
        this.searchArticles();
    }

    renderTagSuggestions() {
        const tagsInput = document.getElementById('tagsInput');
        if (!tagsInput || !tagsInput.parentNode) return;

        const existingSuggestions = tagsInput.parentNode.querySelector('.tag-suggestions');
        if (existingSuggestions) {
            existingSuggestions.remove();
        }

        if (this.popularTags.length === 0) return;

        const suggestionsDiv = document.createElement('div');
        suggestionsDiv.className = 'tag-suggestions';
        suggestionsDiv.innerHTML = `
            <label>よく使うタグ（クリックで自動入力）:</label>
            <div class="suggestion-tags">
                ${this.popularTags.map(tagCount => 
                    `<span class="tag suggestion-tag" onclick="app.handleTagClick('${this.escapeHtml(tagCount.tag)}', 'input')" title="クリックで追加">
                    ${this.escapeHtml(tagCount.tag)}
                </span>`
            ).join('')}
        </div>
    `;

        tagsInput.parentNode.insertBefore(suggestionsDiv, tagsInput.nextSibling);
    }

    showError(message) {
        this.showNotification(message, 'error');
    }

    showSuccess(message) {
        this.showNotification(message, 'success');
    }

    showNotification(message, type) {
        const existing = document.querySelector('.notification');
        if (existing) {
            existing.remove();
        }

        const notification = document.createElement('div');
        notification.className = `notification ${type}`;
        notification.textContent = message;
        notification.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            padding: 12px 20px;
            border-radius: 6px;
            color: white;
            font-weight: 500;
            z-index: 10000;
            background: ${type === 'success' ? '#10b981' : '#ef4444'};
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
            transform: translateX(100%);
            transition: transform 0.3s ease;
        `;
        
        document.body.appendChild(notification);
        
        setTimeout(() => {
            notification.style.transform = 'translateX(0)';
        }, 10);

        setTimeout(() => {
            notification.style.transform = 'translateX(100%)';
            setTimeout(() => {
                if (notification.parentNode) {
                    notification.parentNode.removeChild(notification);
                }
            }, 300);
        }, 3000);
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

//タグ検索の正規化
function normalizeTagString(tagString){
    return tagString
        .replace(/,\s+/g, ',')      // カンマ+スペース → カンマ
        .replace(/\s+,/g, ',')      // スペース+カンマ → カンマ  
        .replace(/\s+/g, ' ')       // 連続スペース → 単一スペース
        .toLowerCase()
        .trim();
}

// アプリ初期化
document.addEventListener('DOMContentLoaded', () => {
    console.log('🚀 Atode アプリケーションを初期化中...');
    
    const checkTauri = () => {
        if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
            console.log('✅ Tauri 2.0 API loaded successfully');
            window.app = new AtodeApp();
        } else {
            console.log('⏳ Waiting for Tauri 2.0 API...');
            setTimeout(checkTauri, 100);
        }
    };
    
    checkTauri();
});