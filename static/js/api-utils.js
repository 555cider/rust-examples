// API Utility Functions



// Helper function to normalize API response
function normalizeAPIResponse(data) {
    let items = [];
    let total = 0;

    if (Array.isArray(data)) {
        items = data;
        total = data.length;
    } else if (data && Array.isArray(data.items)) {
        items = data.items;
        total = data.total || data.items.length;
    } else if (data && data.data && Array.isArray(data.data)) {
        items = data.data;
        total = data.total || data.data.length;
    }

    return { items, total };
}

// Format date
function formatDate(dateString, includeTime = false) {
    if (!dateString) return '-';

    const date = new Date(dateString);
    const options = {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit'
    };

    if (includeTime) {
        options.hour = '2-digit';
        options.minute = '2-digit';
        options.second = '2-digit';
    }

    return date.toLocaleString('ko-KR', options);
}

// Show toast notification
async function showToast(title, message, icon = 'success') {
    if (typeof Swal !== 'undefined') {
        return Swal.fire({
            title: title,
            text: message,
            icon: icon,
            confirmButtonText: '확인'
        });
    } else {
        alert(`${title}: ${message}`);
    }
}

// Show confirm dialog
async function showConfirm(title, message, confirmText = '확인', cancelText = '취소') {
    if (typeof Swal !== 'undefined') {
        return Swal.fire({
            title: title,
            text: message,
            icon: 'warning',
            showCancelButton: true,
            confirmButtonText: confirmText,
            cancelButtonText: cancelText,
            confirmButtonColor: '#dc2626'
        });
    } else {
        return { isConfirmed: confirm(`${title}\n${message}`) };
    }
}

// Global API client instance
// Export helper functions to window
window.normalizeAPIResponse = normalizeAPIResponse;
window.formatDate = formatDate;
window.showToast = showToast;
window.showConfirm = showConfirm;
