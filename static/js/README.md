# JavaScript Utilities Documentation

This directory contains JavaScript utilities for managing loading states, error handling, API calls, and button feedback in the admin dashboard.

## Files Overview

### 1. loading.js - Loading State Management
Provides functions to show/hide loading indicators and manage loading states.

**Usage:**
```javascript
// Show loading overlay
window.loadingManager.show('데이터를 불러오는 중', '잠시만 기다려주세요...');

// Hide loading overlay
window.loadingManager.hide();

// Show loading spinner in an element
window.loadingManager.showInElement(element, 'md'); // sizes: sm, md, lg

// Show skeleton loader in an element
window.loadingManager.showSkeleton(element, 'table'); // types: text, card, table, form

// Set button loading state
window.loadingManager.setButtonLoading(button, true, '처리 중...');
window.loadingManager.setButtonLoading(button, false);
```

### 2. error-handler.js - Error Handling with Retry Logic
Provides user-friendly error messages and automatic retry logic for network errors.

**Usage:**
```javascript
// Get user-friendly error message
const message = window.errorHandler.getMessage(error);

// Execute function with automatic retry
const result = await window.errorHandler.withRetry(async () => {
    return await fetch('/api/endpoint');
}, 3, 1000); // maxRetries, delay

// Show error modal with retry option
window.errorHandler.showErrorModal(error, retryCallback);

// Show inline error in element
window.errorHandler.showInlineError(element, error, retryCallback);

// Handle API error
window.errorHandler.handle(error, {
    showModal: false,
    showToast: true,
    retryCallback: () => loadData(),
    element: containerElement
});
```

### 3. api.js - API Client with Loading and Error Handling
Wraps fetch calls with automatic loading indicators and error handling.

**Usage:**
```javascript
// GET request
const data = await window.apiClient.get('/api/endpoint');

// POST request
const result = await window.apiClient.post('/api/endpoint', { key: 'value' });

// PUT request
await window.apiClient.put('/api/endpoint/1', { key: 'updated' });

// PATCH request
await window.apiClient.patch('/api/endpoint/1', { key: 'patched' });

// DELETE request
await window.apiClient.delete('/api/endpoint/1');

// Disable loading overlay
const data = await window.apiClient.get('/api/endpoint', false);

// Disable auto-retry
const data = await window.apiClient.get('/api/endpoint', true, false);

// Request with element-specific loading
const data = await window.apiClient.requestWithElementLoading(
    '/api/endpoint',
    { method: 'GET' },
    containerElement,
    'skeleton' // or 'spinner'
);
```

### 4. button-feedback.js - Button Click Feedback
Provides visual feedback for button clicks and loading states.

**Usage:**
```javascript
// Set button to loading state
const buttonId = window.buttonFeedback.setLoading(button, '처리 중...');

// Remove loading state
window.buttonFeedback.removeLoading(buttonId);

// Execute async function with button loading
await window.buttonFeedback.withLoading(button, async () => {
    await someAsyncOperation();
}, '저장 중...');

// Show success feedback
await window.buttonFeedback.showSuccess(button, '완료!', 2000);

// Show error feedback
await window.buttonFeedback.showError(button, '오류 발생', 2000);

// Disable/enable button
window.buttonFeedback.disable(button);
window.buttonFeedback.enable(button);
```

## Complete Example

```javascript
// Example: Load data with full error handling and loading states
async function loadData() {
    const container = document.getElementById('data-container');
    const loadButton = document.getElementById('load-button');
    
    try {
        // Show loading in button
        await window.buttonFeedback.withLoading(loadButton, async () => {
            // Make API call with automatic retry
            const data = await window.apiClient.get('/api/data');
            
            // Render data
            renderData(data);
            
            // Show success feedback
            await window.buttonFeedback.showSuccess(loadButton, '완료!');
        }, '로딩 중...');
        
    } catch (error) {
        // Handle error with retry option
        window.errorHandler.handle(error, {
            showModal: true,
            retryCallback: () => loadData()
        });
    }
}

// Example: Form submission with validation
async function submitForm(event) {
    event.preventDefault();
    
    const form = event.target;
    const submitButton = form.querySelector('button[type="submit"]');
    
    try {
        await window.buttonFeedback.withLoading(submitButton, async () => {
            const formData = new FormData(form);
            const data = Object.fromEntries(formData);
            
            // Submit with loading overlay
            const result = await window.apiClient.post('/api/submit', data);
            
            // Show success
            window.toast.success('저장되었습니다.');
            
            // Redirect or update UI
            window.location.href = '/success';
        }, '저장 중...');
        
    } catch (error) {
        window.errorHandler.handle(error, {
            showToast: true
        });
    }
}
```

## Features

### Automatic Loading States
- Loading overlay appears automatically for API calls
- Long-running requests (>2 seconds) show progress message
- Skeleton loaders for better UX

### Error Handling
- User-friendly error messages for all HTTP status codes
- Automatic retry for network errors (max 3 attempts)
- Exponential backoff between retries
- Error modals with retry buttons

### Button Feedback
- Visual click feedback (scale animation)
- Touch feedback for mobile devices
- Loading states with spinners
- Success/error states with icons
- Automatic disable during operations

### API Client
- Consistent error handling across all requests
- Automatic loading indicators
- Retry logic for transient failures
- Support for all HTTP methods

## Browser Support

All utilities are compatible with:
- Chrome (latest 2 versions)
- Firefox (latest 2 versions)
- Safari (latest 2 versions)
- Edge (latest 2 versions)
- Mobile browsers (iOS 13+, Android 8+)
