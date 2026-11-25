/**
 * API Utility with Loading State Management
 * Wraps fetch calls with automatic loading indicators and error handling
 */

class ApiClient {
    constructor() {
        this.baseUrl = '';
        this.defaultHeaders = {
            'Content-Type': 'application/json',
            'X-Requested-With': 'XMLHttpRequest',
        };
    }

    /**
     * Make an API request with loading state
     * @param {string} url - API endpoint
     * @param {Object} options - Fetch options
     * @param {boolean} showLoading - Show loading overlay
     * @param {boolean} autoRetry - Automatically retry on network errors
     * @returns {Promise<any>}
     */
    async request(url, options = {}, showLoading = true, autoRetry = true) {
        const makeRequest = async () => {
            const startTime = Date.now();
            let longLoadingTimer = null;

            try {
                // Show loading indicator
                if (showLoading && window.loadingManager) {
                    window.loadingManager.show();
                }

                // Set up long loading message timer (2 seconds)
                longLoadingTimer = setTimeout(() => {
                    if (showLoading && window.loadingManager) {
                        const messageEl = document.getElementById('loading-message');
                        if (messageEl) {
                            messageEl.textContent = '처리 시간이 예상보다 오래 걸리고 있습니다. 잠시만 더 기다려주세요...';
                        }
                    }
                }, 2000);

                // Make the request
                const response = await fetch(url, {
                    ...options,
                    credentials: 'include',
                    headers: {
                        ...this.defaultHeaders,
                        ...options.headers,
                    },
                });

                // Clear long loading timer
                if (longLoadingTimer) {
                    clearTimeout(longLoadingTimer);
                }

                // Hide loading indicator
                if (showLoading && window.loadingManager) {
                    window.loadingManager.hide();
                }

                // Handle HTTP errors
                if (!response.ok) {
                    // Handle 401 Unauthorized
                    if (response.status === 401) {
                        window.location.href = '/auth/login';
                        // Return a promise that never resolves to prevent further processing
                        return new Promise(() => { });
                    }

                    const error = new Error(`HTTP ${response.status}: ${response.statusText}`);
                    error.status = response.status;
                    error.response = response;

                    // Try to parse error message from response
                    try {
                        const errorData = await response.json();
                        if (errorData.message) {
                            error.message = errorData.message;
                        }
                        // Attach parsed data to response object for compatibility with existing code
                        response.data = errorData;
                        // Also attach to error object
                        error.data = errorData;
                    } catch (e) {
                        // Ignore JSON parse errors
                    }

                    throw error;
                }

                // Parse response
                const contentType = response.headers.get('content-type');
                if (contentType && contentType.includes('application/json')) {
                    return await response.json();
                }

                return await response.text();

            } catch (error) {
                // Clear long loading timer
                if (longLoadingTimer) {
                    clearTimeout(longLoadingTimer);
                }

                // Hide loading indicator on error
                if (showLoading && window.loadingManager) {
                    window.loadingManager.hide();
                }

                throw error;
            }
        };

        // Use retry logic if enabled and error handler is available
        if (autoRetry && window.errorHandler) {
            return await window.errorHandler.withRetry(makeRequest);
        }

        return await makeRequest();
    }

    /**
     * GET request
     */
    async get(url, showLoading = true, autoRetry = true) {
        return this.request(url, { method: 'GET' }, showLoading, autoRetry);
    }

    /**
     * POST request
     */
    async post(url, data, showLoading = true, autoRetry = false) {
        return this.request(url, {
            method: 'POST',
            body: JSON.stringify(data),
        }, showLoading, autoRetry);
    }

    /**
     * PUT request
     */
    async put(url, data, showLoading = true, autoRetry = false) {
        return this.request(url, {
            method: 'PUT',
            body: JSON.stringify(data),
        }, showLoading, autoRetry);
    }

    /**
     * PATCH request
     */
    async patch(url, data, showLoading = true, autoRetry = false) {
        return this.request(url, {
            method: 'PATCH',
            body: JSON.stringify(data),
        }, showLoading, autoRetry);
    }

    /**
     * DELETE request
     */
    async delete(url, showLoading = true, autoRetry = false) {
        return this.request(url, { method: 'DELETE' }, showLoading, autoRetry);
    }

    /**
     * Make a request with element-specific loading state
     * @param {string} url - API endpoint
     * @param {Object} options - Fetch options
     * @param {HTMLElement} element - Element to show loading in
     * @param {string} loaderType - Type of loader (spinner, skeleton)
     * @returns {Promise<any>}
     */
    async requestWithElementLoading(url, options = {}, element, loaderType = 'spinner') {
        if (!element) {
            return this.request(url, options, false);
        }

        try {
            // Show loading in element
            if (loaderType === 'skeleton') {
                window.loadingManager?.showSkeleton(element, 'table');
            } else {
                window.loadingManager?.showInElement(element);
            }

            // Make the request
            const response = await fetch(url, {
                ...options,
                headers: {
                    ...this.defaultHeaders,
                    ...options.headers,
                },
            });

            // Handle HTTP errors
            if (!response.ok) {
                const error = new Error(`HTTP ${response.status}: ${response.statusText}`);
                error.status = response.status;
                error.response = response;
                throw error;
            }

            // Parse response
            const contentType = response.headers.get('content-type');
            if (contentType && contentType.includes('application/json')) {
                return await response.json();
            }

            return await response.text();

        } catch (error) {
            throw error;
        }
    }
}

// Create global instance
const apiClient = new ApiClient();

// Export for use in other scripts
window.apiClient = apiClient;
