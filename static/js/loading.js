/**
 * Loading State Management Utility
 * Provides functions to show/hide loading indicators and manage loading states
 */

class LoadingManager {
    constructor() {
        this.overlay = null;
        this.longLoadingTimer = null;
        this.longLoadingThreshold = 2000; // 2 seconds
    }

    /**
     * Initialize the loading overlay
     */
    init() {
        this.overlay = document.getElementById('loading-overlay');
        if (!this.overlay) {
            console.warn('Loading overlay not found in DOM');
        }
    }

    /**
     * Show loading overlay
     * @param {string} title - Loading title
     * @param {string} message - Loading message
     */
    show(title = '데이터를 불러오는 중', message = '잠시만 기다려주세요...') {
        if (!this.overlay) this.init();
        
        if (this.overlay) {
            const titleEl = document.getElementById('loading-title');
            const messageEl = document.getElementById('loading-message');
            
            if (titleEl) titleEl.textContent = title;
            if (messageEl) messageEl.textContent = message;
            
            this.overlay.classList.remove('hidden');
            
            // Set up long loading message
            this.longLoadingTimer = setTimeout(() => {
                if (messageEl) {
                    messageEl.textContent = '처리 시간이 예상보다 오래 걸리고 있습니다. 잠시만 더 기다려주세요...';
                }
            }, this.longLoadingThreshold);
        }
    }

    /**
     * Hide loading overlay
     */
    hide() {
        if (this.overlay) {
            this.overlay.classList.add('hidden');
        }
        
        // Clear long loading timer
        if (this.longLoadingTimer) {
            clearTimeout(this.longLoadingTimer);
            this.longLoadingTimer = null;
        }
    }

    /**
     * Show loading spinner in a specific element
     * @param {HTMLElement} element - Target element
     * @param {string} size - Spinner size (sm, md, lg)
     */
    showInElement(element, size = 'md') {
        if (!element) return;
        
        const sizeClasses = {
            sm: 'h-5 w-5',
            md: 'h-8 w-8',
            lg: 'h-12 w-12'
        };
        
        const spinner = document.createElement('div');
        spinner.className = 'loading-spinner-inline flex items-center justify-center py-8';
        spinner.innerHTML = `
            <div class="animate-spin rounded-full ${sizeClasses[size]} border-t-2 border-b-2 border-primary-600"></div>
        `;
        
        element.innerHTML = '';
        element.appendChild(spinner);
    }

    /**
     * Show skeleton loader in a specific element
     * @param {HTMLElement} element - Target element
     * @param {string} type - Skeleton type (text, card, table, form)
     */
    showSkeleton(element, type = 'text') {
        if (!element) return;
        
        const skeletons = {
            text: `
                <div class="skeleton-loader animate-pulse space-y-4">
                    <div class="h-4 bg-gray-200 rounded w-3/4"></div>
                    <div class="h-4 bg-gray-200 rounded"></div>
                    <div class="h-4 bg-gray-200 rounded w-5/6"></div>
                </div>
            `,
            card: `
                <div class="skeleton-card animate-pulse">
                    <div class="bg-white rounded-xl border border-gray-200 p-6 space-y-4">
                        <div class="h-6 bg-gray-200 rounded w-1/2"></div>
                        <div class="space-y-2">
                            <div class="h-4 bg-gray-200 rounded"></div>
                            <div class="h-4 bg-gray-200 rounded w-5/6"></div>
                            <div class="h-4 bg-gray-200 rounded w-3/4"></div>
                        </div>
                    </div>
                </div>
            `,
            table: `
                <div class="skeleton-table animate-pulse">
                    <div class="bg-white rounded-xl border border-gray-200 overflow-hidden">
                        <div class="bg-gray-50 px-6 py-3 border-b border-gray-200">
                            <div class="flex space-x-4">
                                <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                            </div>
                        </div>
                        <div class="divide-y divide-gray-200">
                            ${Array(3).fill().map(() => `
                                <div class="px-6 py-4">
                                    <div class="flex space-x-4">
                                        <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                        <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                        <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                        <div class="h-4 bg-gray-200 rounded w-1/4"></div>
                                    </div>
                                </div>
                            `).join('')}
                        </div>
                    </div>
                </div>
            `,
            form: `
                <div class="skeleton-form animate-pulse space-y-4">
                    <div>
                        <div class="h-4 bg-gray-200 rounded w-1/4 mb-2"></div>
                        <div class="h-10 bg-gray-200 rounded"></div>
                    </div>
                    <div>
                        <div class="h-4 bg-gray-200 rounded w-1/3 mb-2"></div>
                        <div class="h-10 bg-gray-200 rounded"></div>
                    </div>
                    <div>
                        <div class="h-4 bg-gray-200 rounded w-1/5 mb-2"></div>
                        <div class="h-24 bg-gray-200 rounded"></div>
                    </div>
                </div>
            `
        };
        
        element.innerHTML = skeletons[type] || skeletons.text;
    }

    /**
     * Set button loading state
     * @param {HTMLButtonElement} button - Button element
     * @param {boolean} loading - Loading state
     * @param {string} loadingText - Text to show while loading
     */
    setButtonLoading(button, loading, loadingText = '처리 중...') {
        if (!button) return;
        
        if (loading) {
            button.disabled = true;
            button.dataset.originalText = button.innerHTML;
            button.innerHTML = `
                <div class="flex items-center justify-center space-x-2">
                    <div class="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2 border-white"></div>
                    <span>${loadingText}</span>
                </div>
            `;
            button.classList.add('opacity-75', 'cursor-not-allowed');
        } else {
            button.disabled = false;
            button.innerHTML = button.dataset.originalText || button.innerHTML;
            button.classList.remove('opacity-75', 'cursor-not-allowed');
            delete button.dataset.originalText;
        }
    }
}

// Create global instance
const loadingManager = new LoadingManager();

// Initialize on DOM ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => loadingManager.init());
} else {
    loadingManager.init();
}

// Export for use in other scripts
window.loadingManager = loadingManager;
