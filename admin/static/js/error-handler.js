/**
 * Error Handling Utility
 * Provides user-friendly error messages and retry logic
 */

class ErrorHandler {
    constructor() {
        this.maxRetries = 3;
        this.retryDelay = 1000; // 1 second
        this.errorMessages = {
            // Network errors
            'network_error': '네트워크 연결을 확인해주세요.',
            'timeout': '요청 시간이 초과되었습니다. 다시 시도해주세요.',
            
            // HTTP status codes
            400: '잘못된 요청입니다. 입력 값을 확인해주세요.',
            401: '로그인이 필요합니다.',
            403: '접근 권한이 없습니다.',
            404: '요청한 리소스를 찾을 수 없습니다.',
            409: '이미 존재하는 데이터입니다.',
            422: '입력 값을 확인해주세요.',
            429: '너무 많은 요청을 보냈습니다. 잠시 후 다시 시도해주세요.',
            500: '서버 오류가 발생했습니다. 잠시 후 다시 시도해주세요.',
            502: '서버에 연결할 수 없습니다.',
            503: '서비스를 일시적으로 사용할 수 없습니다.',
            504: '서버 응답 시간이 초과되었습니다.',
            
            // Default
            'default': '오류가 발생했습니다. 다시 시도해주세요.'
        };
    }

    /**
     * Get user-friendly error message
     * @param {Error|number|string} error - Error object, status code, or error type
     * @returns {string} User-friendly error message
     */
    getMessage(error) {
        if (typeof error === 'number') {
            return this.errorMessages[error] || this.errorMessages['default'];
        }
        
        if (typeof error === 'string') {
            return this.errorMessages[error] || error;
        }
        
        if (error instanceof Error) {
            // Check if it's a network error
            if (error.message.includes('Failed to fetch') || error.message.includes('NetworkError')) {
                return this.errorMessages['network_error'];
            }
            
            // Check if it has a status code
            if (error.status) {
                return this.errorMessages[error.status] || this.errorMessages['default'];
            }
            
            // Return the error message or default
            return error.message || this.errorMessages['default'];
        }
        
        return this.errorMessages['default'];
    }

    /**
     * Check if error is retryable
     * @param {Error} error - Error object
     * @returns {boolean} Whether the error is retryable
     */
    isRetryable(error) {
        // Network errors are retryable
        if (error.message && (error.message.includes('Failed to fetch') || error.message.includes('NetworkError'))) {
            return true;
        }
        
        // Certain HTTP status codes are retryable
        const retryableStatuses = [408, 429, 500, 502, 503, 504];
        if (error.status && retryableStatuses.includes(error.status)) {
            return true;
        }
        
        return false;
    }

    /**
     * Execute a function with automatic retry on failure
     * @param {Function} fn - Async function to execute
     * @param {number} maxRetries - Maximum number of retries (default: 3)
     * @param {number} delay - Delay between retries in ms (default: 1000)
     * @returns {Promise<any>} Result of the function
     */
    async withRetry(fn, maxRetries = this.maxRetries, delay = this.retryDelay) {
        let lastError;
        
        for (let attempt = 0; attempt <= maxRetries; attempt++) {
            try {
                return await fn();
            } catch (error) {
                lastError = error;
                
                // Don't retry if error is not retryable
                if (!this.isRetryable(error)) {
                    throw error;
                }
                
                // Don't retry if this was the last attempt
                if (attempt === maxRetries) {
                    throw error;
                }
                
                // Wait before retrying (exponential backoff)
                const waitTime = delay * Math.pow(2, attempt);
                await this.sleep(waitTime);
            }
        }
        
        throw lastError;
    }

    /**
     * Sleep for specified milliseconds
     * @param {number} ms - Milliseconds to sleep
     * @returns {Promise<void>}
     */
    sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    /**
     * Show error modal with retry option
     * @param {Error|string} error - Error to display
     * @param {Function} retryCallback - Function to call when retry is clicked
     */
    showErrorModal(error, retryCallback = null) {
        const message = this.getMessage(error);
        const isRetryable = error instanceof Error && this.isRetryable(error);
        
        // Create modal HTML
        const modalHtml = `
            <div id="error-modal" class="fixed inset-0 bg-black/30 backdrop-blur-sm flex items-center justify-center z-50">
                <div class="bg-white p-8 rounded-2xl shadow-2xl max-w-md w-full mx-4">
                    <div class="flex flex-col items-center">
                        <div class="flex items-center justify-center w-16 h-16 bg-error-100 rounded-full mb-4">
                            <i class="fas fa-exclamation-triangle text-error-600 text-2xl"></i>
                        </div>
                        <h3 class="text-lg font-semibold text-gray-900 mb-2">오류가 발생했습니다</h3>
                        <p class="text-gray-600 text-center mb-6">${message}</p>
                        <div class="flex space-x-3 w-full">
                            ${isRetryable && retryCallback ? `
                                <button onclick="window.errorHandler.handleRetry()" class="btn-primary flex-1">
                                    <i class="fas fa-redo mr-2"></i> 다시 시도
                                </button>
                            ` : ''}
                            <button onclick="window.errorHandler.closeErrorModal()" class="btn-secondary flex-1">
                                닫기
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        `;
        
        // Remove existing modal if any
        this.closeErrorModal();
        
        // Add modal to body
        document.body.insertAdjacentHTML('beforeend', modalHtml);
        
        // Store retry callback
        if (retryCallback) {
            this.currentRetryCallback = retryCallback;
        }
    }

    /**
     * Close error modal
     */
    closeErrorModal() {
        const modal = document.getElementById('error-modal');
        if (modal) {
            modal.remove();
        }
        this.currentRetryCallback = null;
    }

    /**
     * Handle retry button click
     */
    async handleRetry() {
        this.closeErrorModal();
        
        if (this.currentRetryCallback) {
            try {
                await this.currentRetryCallback();
            } catch (error) {
                console.error('Retry failed:', error);
                this.showErrorModal(error, this.currentRetryCallback);
            }
        }
    }

    /**
     * Show inline error message in an element
     * @param {HTMLElement} element - Element to show error in
     * @param {Error|string} error - Error to display
     * @param {Function} retryCallback - Function to call when retry is clicked
     */
    showInlineError(element, error, retryCallback = null) {
        if (!element) return;
        
        const message = this.getMessage(error);
        const isRetryable = error instanceof Error && this.isRetryable(error);
        
        element.innerHTML = `
            <div class="flex flex-col items-center justify-center py-8 px-4">
                <div class="flex items-center justify-center w-12 h-12 bg-error-100 rounded-full mb-3">
                    <i class="fas fa-exclamation-triangle text-error-600 text-xl"></i>
                </div>
                <p class="text-sm text-gray-600 text-center mb-4">${message}</p>
                ${isRetryable && retryCallback ? `
                    <button onclick="(${retryCallback.toString()})()" class="btn-primary btn-sm">
                        <i class="fas fa-redo mr-2"></i> 다시 시도
                    </button>
                ` : ''}
            </div>
        `;
    }

    /**
     * Handle API error and show appropriate message
     * @param {Error} error - Error object
     * @param {Object} options - Options for error handling
     * @param {boolean} options.showModal - Show error modal (default: false)
     * @param {boolean} options.showToast - Show toast notification (default: true)
     * @param {Function} options.retryCallback - Retry callback function
     * @param {HTMLElement} options.element - Element to show inline error
     */
    handle(error, options = {}) {
        const {
            showModal = false,
            showToast = true,
            retryCallback = null,
            element = null
        } = options;
        
        console.error('Error:', error);
        
        // Show inline error if element is provided
        if (element) {
            this.showInlineError(element, error, retryCallback);
            return;
        }
        
        // Show modal if requested
        if (showModal) {
            this.showErrorModal(error, retryCallback);
            return;
        }
        
        // Show toast notification by default
        if (showToast && window.toast) {
            const message = this.getMessage(error);
            window.toast.error(message);
        }
    }
}

// Create global instance
const errorHandler = new ErrorHandler();

// Export for use in other scripts
window.errorHandler = errorHandler;
