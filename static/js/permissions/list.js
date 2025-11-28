/**
 * Permission List Page Script
 */

// Global state
const PermissionList = {
    currentPage: 1,
    itemsPerPage: 10,
    searchTimeout: null,
    sortBy: 'id',
    order: 'asc'
};

document.addEventListener('DOMContentLoaded', function () {
    initializePage();
    updateSortIcons();
});

function initializePage() {
    loadPermissions();

    const searchInput = document.getElementById('search');
    if (searchInput) {
        searchInput.addEventListener('input', handleSearch);
        searchInput.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') {
                clearTimeout(PermissionList.searchTimeout);
                PermissionList.currentPage = 1;
                loadPermissions();
            }
        });
    }

    const searchButton = document.getElementById('searchButton');
    if (searchButton) {
        searchButton.addEventListener('click', function () {
            clearTimeout(PermissionList.searchTimeout);
            PermissionList.currentPage = 1;
            loadPermissions();
        });
    }

    document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape') {
            closePermissionModal();
        }
    });

    // Initialize form submit handler
    const form = document.getElementById('permissionForm');
    if (form) {
        form.addEventListener('submit', handleFormSubmit);
    }
}

function handleSearch() {
    clearTimeout(PermissionList.searchTimeout);
    PermissionList.searchTimeout = setTimeout(() => {
        PermissionList.currentPage = 1;
        loadPermissions();
    }, 300);
}

// Modal Functions
function openPermissionModal(permissionId = null) {
    const modal = document.getElementById('permissionModal');
    const form = document.getElementById('permissionForm');
    const modalTitle = document.getElementById('modalTitle');

    if (!modal || !form) return;

    form.reset();
    document.querySelectorAll('[id$="-error"]').forEach(el => el.textContent = '');

    if (permissionId) {
        modalTitle.textContent = '권한 정보 수정';
        document.getElementById('permissionId').value = permissionId;
        loadPermissionData(permissionId);
    } else {
        modalTitle.textContent = '새 권한 추가';
        document.getElementById('permissionId').value = '';
    }

    modal.classList.add('active');
    document.body.style.overflow = 'hidden';
}

function closePermissionModal() {
    const modal = document.getElementById('permissionModal');
    if (modal) {
        modal.classList.remove('active');
    }
    document.body.style.overflow = '';
}

async function loadPermissionData(permissionId) {
    try {
        const permission = await window.apiClient.get(`/api/permissions/detail?id=${permissionId}`);
        document.getElementById('code').value = permission.code || '';
        document.getElementById('name').value = permission.name || '';
    } catch (error) {
        console.error('Error loading permission:', error);
        Swal.fire('오류', '권한 정보를 불러오는데 실패했습니다.', 'error');
    }
}

async function handleFormSubmit(e) {
    e.preventDefault();

    const permissionId = document.getElementById('permissionId').value;
    const isNew = !permissionId;

    const formData = {
        code: document.getElementById('code').value.trim(),
        name: document.getElementById('name').value.trim()
    };

    const url = isNew ? '/api/permissions' : '/api/permissions/update';
    const method = isNew ? 'POST' : 'PUT';

    try {
        let data;
        if (isNew) {
            data = await window.apiClient.post('/api/permissions', formData);
        } else {
            data = await window.apiClient.put(`/api/permissions/update?id=${permissionId}`, formData);
        }

        await Swal.fire({
            title: '성공!',
            text: isNew ? '새 권한이 추가되었습니다.' : '권한 정보가 수정되었습니다.',
            icon: 'success',
            confirmButtonText: '확인'
        });

        closePermissionModal();
        loadPermissions();
    } catch (error) {
        console.error('Error:', error);

        if (error.response?.data) {
            const errors = error.response.data.errors || {};
            document.querySelectorAll('[id$="-error"]').forEach(el => el.textContent = '');

            for (const [field, message] of Object.entries(errors)) {
                const errorElement = document.getElementById(`${field}-error`);
                if (errorElement) {
                    errorElement.textContent = Array.isArray(message) ? message[0] : message;
                }
            }

            if (Object.keys(errors).length === 0) {
                const errorMsg = error.response.data.message || '오류가 발생했습니다.';
                Swal.fire('오류', errorMsg, 'error');
            }
        } else {
            Swal.fire('오류', '요청을 처리하는 중 오류가 발생했습니다.', 'error');
        }
    }
}

function editPermission(permissionId) {
    openPermissionModal(permissionId);
}

async function deletePermission(permissionId) {
    const result = await Swal.fire({
        title: '권한 삭제',
        text: '정말 이 권한을 삭제하시겠습니까?',
        icon: 'warning',
        showCancelButton: true,
        confirmButtonText: '삭제',
        cancelButtonText: '취소',
        confirmButtonColor: '#dc2626'
    });

    if (!result.isConfirmed) return;

    try {
        await window.apiClient.delete(`/api/permissions/delete?id=${permissionId}`);

        await Swal.fire('삭제 완료', '권한이 삭제되었습니다.', 'success');
        loadPermissions();
    } catch (error) {
        console.error('Error:', error);
        Swal.fire('오류', '권한 삭제 중 오류가 발생했습니다.', 'error');
    }
}

// Data Loading & Rendering
async function loadPermissions() {
    const tbody = document.getElementById('permissions-table-body');
    if (!tbody) return;

    tbody.innerHTML = `
        <tr>
            <td colspan="5" class="px-6 py-8 text-center text-gray-500">
                <i class="fas fa-spinner fa-spin mr-2"></i>데이터를 불러오는 중...
            </td>
        </tr>`;

    const search = document.getElementById('search').value || '';

    try {
        const params = new URLSearchParams({
            page: PermissionList.currentPage,
            limit: PermissionList.itemsPerPage,
            search: search,
            sort_by: PermissionList.sortBy,
            order: PermissionList.order
        });

        // Use the global apiClient
        const data = await window.apiClient.get(`/api/permissions?${params.toString()}`);

        let permissions = [];
        let total = 0;

        if (Array.isArray(data)) {
            permissions = data;
            total = data.length;
        } else if (data && Array.isArray(data.items)) {
            permissions = data.items;
            total = data.total || data.items.length;
        } else if (data && data.data && Array.isArray(data.data)) {
            permissions = data.data;
            total = data.total || data.data.length;
        }

        renderPermissions(permissions);
        updatePagination(Number(total), PermissionList.currentPage, PermissionList.itemsPerPage);
    } catch (error) {
        console.error('Error loading permissions:', error);
        tbody.innerHTML = `
            <tr>
                <td colspan="5" class="px-6 py-8 text-center text-red-500">
                    <i class="fas fa-exclamation-triangle mr-2"></i>데이터를 불러오는데 실패했습니다
                </td>
            </tr>`;
    }
}

function renderPermissions(permissions) {
    const tbody = document.getElementById('permissions-table-body');
    const mobileContainer = document.getElementById('mobile-permissions');

    if (!tbody) return;

    // Clear existing content
    tbody.innerHTML = '';
    if (mobileContainer) {
        mobileContainer.innerHTML = '';
    }

    if (!permissions || permissions.length === 0) {
        tbody.innerHTML = `
            <tr>
                <td colspan="5" class="px-6 py-8 text-center text-gray-500">
                    <i class="fas fa-inbox text-2xl mb-2"></i>
                    <p>표시할 권한이 없습니다</p>
                </td>
            </tr>`;
        if (mobileContainer) {
            mobileContainer.innerHTML = '<div class="text-center py-8 text-gray-500">표시할 권한이 없습니다.</div>';
        }
        return;
    }

    // Render desktop table
    tbody.innerHTML = permissions.map(permission => {
        const createdAt = permission.created_at ? new Date(permission.created_at).toLocaleDateString('ko-KR') : '-';

        return `
            <tr class="hover:bg-gray-50 transition-colors">
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900">${permission.id}</td>
                <td class="px-6 py-4 whitespace-nowrap">
                    <code class="text-xs bg-gray-100 px-2 py-1 rounded font-mono">${permission.code || '-'}</code>
                </td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900">${permission.name || '-'}</td>
                <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-center">${createdAt}</td>
                <td class="px-6 py-4 whitespace-nowrap text-right text-sm font-medium space-x-3">
                    <button onclick="editPermission(${permission.id})" class="text-primary-600 hover:text-primary-900">
                        <i class="fas fa-edit mr-1"></i>수정
                    </button>
                    <button onclick="deletePermission(${permission.id})" class="text-red-600 hover:text-red-900">
                        <i class="fas fa-trash mr-1"></i>삭제
                    </button>
                </td>
            </tr>
        `;
    }).join('');

    // Render mobile cards
    if (mobileContainer) {
        mobileContainer.innerHTML = permissions.map(permission => {
            const createdAt = permission.created_at ? new Date(permission.created_at).toLocaleDateString('ko-KR') : '-';

            return `
                <div class="bg-white rounded-lg p-4 shadow-sm border border-gray-100 mb-4">
                    <div class="flex items-start justify-between mb-3 pb-3 border-b border-gray-100">
                        <div class="flex-1">
                            <div class="flex items-center gap-2 mb-1">
                                <span class="badge badge-info text-xs">#${permission.id}</span>
                                <code class="text-xs bg-gray-100 px-2 py-0.5 rounded font-mono">${permission.code || '-'}</code>
                            </div>
                            <h3 class="text-sm font-semibold text-gray-900">${permission.name || '-'}</h3>
                        </div>
                    </div>
                    <div class="space-y-2 mb-3">
                        <div class="flex items-center text-xs text-gray-500">
                            <i class="fas fa-calendar-alt mr-2 w-4"></i>
                            <span>${createdAt}</span>
                        </div>
                    </div>
                    <div class="flex justify-end gap-2 pt-3 border-t border-gray-100">
                        <button onclick="editPermission(${permission.id})"
                                class="text-primary-600 hover:text-primary-900 font-medium text-sm px-3 py-1.5">
                            <i class="fas fa-edit mr-1"></i> 수정
                        </button>
                        <button onclick="deletePermission(${permission.id})"
                                class="text-red-600 hover:text-red-900 font-medium text-sm px-3 py-1.5">
                            <i class="fas fa-trash mr-1"></i> 삭제
                        </button>
                    </div>
                </div>
            `;
        }).join('');
    }
}

function updatePagination(total, currentPage, itemsPerPage) {
    const totalPages = Math.ceil(total / itemsPerPage);
    const info = document.getElementById('pagination-info');
    const buttons = document.getElementById('pagination-buttons');

    if (info) {
        info.textContent = `총 ${total}개 중 ${(currentPage - 1) * itemsPerPage + 1} - ${Math.min(currentPage * itemsPerPage, total)}개 표시`;
    }

    if (buttons) {
        let html = '';

        if (currentPage > 1) {
            html += `<button onclick="changePage(${currentPage - 1})" class="btn-secondary btn-sm">
                <i class="fas fa-chevron-left"></i>
            </button>`;
        }

        for (let i = 1; i <= totalPages; i++) {
            if (i === currentPage) {
                html += `<button class="btn-primary btn-sm">${i}</button>`;
            } else {
                html += `<button onclick="changePage(${i})" class="btn-secondary btn-sm">${i}</button>`;
            }
        }

        if (currentPage < totalPages) {
            html += `<button onclick="changePage(${currentPage + 1})" class="btn-secondary btn-sm">
                <i class="fas fa-chevron-right"></i>
            </button>`;
        }

        buttons.innerHTML = html;
    }
}

function changePage(page) {
    PermissionList.currentPage = page;
    loadPermissions();
    window.scrollTo({ top: 0, behavior: 'smooth' });
}

function handleSort(field) {
    if (PermissionList.sortBy === field) {
        PermissionList.order = PermissionList.order === 'asc' ? 'desc' : 'asc';
    } else {
        PermissionList.sortBy = field;
        PermissionList.order = 'asc';
    }
    PermissionList.currentPage = 1;
    updateSortIcons();
    loadPermissions();
}

function updateSortIcons() {
    const headers = document.querySelectorAll('th[data-sort]');
    headers.forEach(header => {
        const field = header.getAttribute('data-sort');
        const icon = header.querySelector('.sort-icon');

        if (field === PermissionList.sortBy) {
            if (PermissionList.order === 'asc') {
                icon.className = 'sort-icon fas fa-sort-up text-primary-600 ml-1';
            } else {
                icon.className = 'sort-icon fas fa-sort-down text-primary-600 ml-1';
            }
        } else {
            icon.className = 'sort-icon fas fa-sort text-gray-300 ml-1';
        }
    });
}

// Export functions to window for HTML event handlers
window.openPermissionModal = openPermissionModal;
window.closePermissionModal = closePermissionModal;
window.editPermission = editPermission;
window.deletePermission = deletePermission;
window.changePage = changePage;
window.handleSort = handleSort;
