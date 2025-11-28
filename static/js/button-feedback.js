/**
 * Button Feedback Utility
 * Provides visual feedback for button clicks and loading states
 */

class ButtonFeedback {
    constructor() {
        this.activeButtons = new Map();
    }

    /**
     * Initialize button feedback for all buttons
     */
    init() {
        // Add click feedback to all buttons
        document.addEventListener('click', (e) => {
            const button = e.target.closest('button, a.btn, a.btn-primary, a.btn-secondary, a.btn-success, a.btn-warning, a.btn-danger, a.btn-info');
            if (button && !button.disabled) {
                this.addClickFeedback(button);
            }
        }, true);

        // Add touch feedback for mobile
        document.addEventListener('touchstart', (e) => {
            const button = e.target.closest('button, a.btn, a.btn-primary, a.btn-secondary, a.btn-success, a.btn-warning, a.btn-danger, a.btn-info');
            if (button && !button.disabled) {
                this.addTouchFeedback(button);
            }
        }, { passive: true });

        document.addEventListener('touchend', (e) => {
            const button = e.target.closest('button, a.btn, a.btn-primary, a.btn-secondary, a.btn-success, a.btn-warning, a.btn-danger, a.btn-info');
            if (button) {
                this.removeTouchFeedback(button);
            }
        }, { passive: true });
    }

    /**
     * Add click feedback animation
     * @param {HTMLElement} button - Button element
     */
    addClickFeedback(button) {
        // Add scale animation
        button.style.transform = 'scale(0.95)';
        
        setTimeout(() => {
            button.style.transform = '';
        }, 150);
    }

    /**
     * Add touch feedback for mobile
     * @param {HTMLElement} button - Button element
     */
    addTouchFeedback(button) {
        button.classList.add('opacity-75');
    }

    /**
     * Remove touch feedback
     * @param {HTMLElement} button - Button element
     */
    removeTouchFeedback(button) {
        button.classList.remove('opacity-75');
    }

    /**
     * Set button to loading state
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     * @param {string} loadingText - Text to show while loading
     * @returns {string} Button ID for later reference
     */
    setLoading(buttonOrSelector, loadingText = '처리 중...') {
        const button = typeof buttonOrSelector === 'string' 
            ? document.querySelector(buttonOrSelector) 
            : buttonOrSelector;
            
        if (!button) {
            console.warn('Button not found:', buttonOrSelector);
            return null;
        }

        // Generate unique ID if not exists
        if (!button.id) {
            button.id = `btn-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        }

        // Store original state
        this.activeButtons.set(button.id, {
            disabled: button.disabled,
            innerHTML: button.innerHTML,
            classList: Array.from(button.classList)
        });

        // Set loading state
        button.disabled = true;
        button.innerHTML = `
            <span class="flex items-center justify-center space-x-2">
                <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <span>${loadingText}</span>
            </span>
        `;
        button.classList.add('opacity-75', 'cursor-not-allowed');

        return button.id;
    }

    /**
     * Remove loading state from button
     * @param {HTMLElement|string} buttonOrSelector - Button element, selector, or button ID
     */
    removeLoading(buttonOrSelector) {
        let button;
        
        if (typeof buttonOrSelector === 'string') {
            // Try as selector first, then as ID
            button = document.querySelector(buttonOrSelector) || document.getElementById(buttonOrSelector);
        } else {
            button = buttonOrSelector;
        }
        
        if (!button || !button.id) {
            console.warn('Button not found or has no ID:', buttonOrSelector);
            return;
        }

        const originalState = this.activeButtons.get(button.id);
        if (!originalState) {
            console.warn('No original state found for button:', button.id);
            return;
        }

        // Restore original state
        button.disabled = originalState.disabled;
        button.innerHTML = originalState.innerHTML;
        button.className = originalState.classList.join(' ');

        // Clean up
        this.activeButtons.delete(button.id);
    }

    /**
     * Execute an async function with button loading state
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     * @param {Function} asyncFn - Async function to execute
     * @param {string} loadingText - Text to show while loading
     * @returns {Promise<any>} Result of the async function
     */
    async withLoading(buttonOrSelector, asyncFn, loadingText = '처리 중...') {
        const buttonId = this.setLoading(buttonOrSelector, loadingText);
        
        try {
            const result = await asyncFn();
            return result;
        } finally {
            if (buttonId) {
                this.removeLoading(buttonId);
            }
        }
    }

    /**
     * Add success feedback to button
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     * @param {string} successText - Success message
     * @param {number} duration - Duration to show success state (ms)
     */
    async showSuccess(buttonOrSelector, successText = '완료!', duration = 2000) {
        const button = typeof buttonOrSelector === 'string' 
            ? document.querySelector(buttonOrSelector) 
            : buttonOrSelector;
            
        if (!button) return;

        const originalHTML = button.innerHTML;
        const originalDisabled = button.disabled;

        button.disabled = true;
        button.innerHTML = `
            <span class="flex items-center justify-center space-x-2">
                <i class="fas fa-check"></i>
                <span>${successText}</span>
            </span>
        `;
        button.classList.add('bg-success-600', 'hover:bg-success-700');

        await new Promise(resolve => setTimeout(resolve, duration));

        button.innerHTML = originalHTML;
        button.disabled = originalDisabled;
        button.classList.remove('bg-success-600', 'hover:bg-success-700');
    }

    /**
     * Add error feedback to button
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     * @param {string} errorText - Error message
     * @param {number} duration - Duration to show error state (ms)
     */
    async showError(buttonOrSelector, errorText = '오류 발생', duration = 2000) {
        const button = typeof buttonOrSelector === 'string' 
            ? document.querySelector(buttonOrSelector) 
            : buttonOrSelector;
            
        if (!button) return;

        const originalHTML = button.innerHTML;

        button.innerHTML = `
            <span class="flex items-center justify-center space-x-2">
                <i class="fas fa-exclamation-triangle"></i>
                <span>${errorText}</span>
            </span>
        `;
        button.classList.add('bg-error-600', 'hover:bg-error-700');

        await new Promise(resolve => setTimeout(resolve, duration));

        button.innerHTML = originalHTML;
        button.classList.remove('bg-error-600', 'hover:bg-error-700');
    }

    /**
     * Disable button
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     */
    disable(buttonOrSelector) {
        const button = typeof buttonOrSelector === 'string' 
            ? document.querySelector(buttonOrSelector) 
            : buttonOrSelector;
            
        if (button) {
            button.disabled = true;
            button.classList.add('opacity-50', 'cursor-not-allowed');
        }
    }

    /**
     * Enable button
     * @param {HTMLElement|string} buttonOrSelector - Button element or selector
     */
    enable(buttonOrSelector) {
        const button = typeof buttonOrSelector === 'string' 
            ? document.querySelector(buttonOrSelector) 
            : buttonOrSelector;
            
        if (button) {
            button.disabled = false;
            button.classList.remove('opacity-50', 'cursor-not-allowed');
        }
    }
}

// Create global instance
const buttonFeedback = new ButtonFeedback();

// Initialize on DOM ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => buttonFeedback.init());
} else {
    buttonFeedback.init();
}

// Export for use in other scripts
window.buttonFeedback = buttonFeedback;
