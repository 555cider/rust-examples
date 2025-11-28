/**
 * User List Page Script
 */

const UserList = {
    currentPage: 1,
    itemsPerPage: 10,
};

let roleMap = {};

document.addEventListener('DOMContentLoaded', function () {
    initializePage();
});

// Also try to initialize if DOM is already loaded
if (document.readyState === 'complete' || document.readyState === 'interactive') {
    setTimeout(initializePage, 1);
}

async function initializePage() {
    const tbody = document.getElementById('users-table-body');
    if (!tbody) {
        return;
    }

    const mobileContainer = document.getElementById('mobile-users');

    // Add loading state
    tbody.innerHTML = `
        <tr>
            <td colspan="7" class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-center">
                <i class="fas fa-spinner fa-spin mr-2"></i> 사용자 목록을 불러오는 중입니다...
            </td>
        </tr>`;

    if (mobileContainer) {
        mobileContainer.innerHTML = '<div class="text-center py-8 text-gray-500"><i class="fas fa-spinner fa-spin mr-2"></i> 사용자 목록을 불러오는 중...</div>';
    }

    // Load initial data
    await fetchRoles();
    loadUsers();

    // Add event listeners
    const searchInput = document.getElementById('search');
    const searchButton = document.getElementById('search-button');
    const statusFilter = document.getElementById('status');

    if (searchInput) {
        searchInput.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') {
                UserList.currentPage = 1;
                loadUsers();
            }
        });
    }

    if (searchButton) {
        searchButton.addEventListener('click', function () {
            UserList.currentPage = 1;
            loadUsers();
        });
    }

    if (statusFilter) {
        statusFilter.addEventListener('change', function () {
            UserList.currentPage = 1;
            loadUsers();
        });
    }
}

async function loadUsers() {
    const tbody = document.getElementById('users-table-body');
    const mobileContainer = document.getElementById('mobile-users');

    if (!tbody) return;

    // Show loading state
    tbody.innerHTML = `
        <tr>
            <td colspan="7" class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-center">
                <i class="fas fa-spinner fa-spin mr-2"></i> 서버에서 데이터를 가져오는 중...
            </td>
        </tr>`;

    if (mobileContainer) {
        mobileContainer.innerHTML = '<div class="text-center py-8 text-gray-500"><i class="fas fa-spinner fa-spin mr-2"></i> 서버에서 데이터를 가져오는 중...</div>';
    }

    // Get search and filter values
    const search = document.getElementById('search')?.value || '';
    const status = document.getElementById('status')?.value || 'all';

    try {
        // Build query parameters
        const params = new URLSearchParams({
            page: UserList.currentPage,
            limit: UserList.itemsPerPage,
            search: search,
            status: status === 'all' ? '' : status
        });

        // Use the global apiClient
        const data = await window.apiClient.get(`/api/users?${params.toString()}`);

        // Check if data is valid
        if (!data) {
            throw new Error('서버로부터 유효하지 않은 응답을 받았습니다.');
        }

        // Handle different response formats
        let users = [];
        let total = 0;

        if (Array.isArray(data)) {
            users = data;
            total = data.length;
        } else if (data && Array.isArray(data.items)) {
            users = data.items;
            total = data.total || data.items.length;
        } else if (data && data.data && Array.isArray(data.data)) {
            users = data.data;
            total = data.total || data.data.length;
        }

        // Render users
        renderUsers(users);

        // Update pagination
        updatePagination(
            Number(total),
            UserList.currentPage,
            UserList.itemsPerPage
        );
    } catch (error) {
        console.error('Error loading users:', error);
        const errorMessage = error.message || '사용자 목록을 불러오는 중 오류가 발생했습니다.';
        showError(`오류: ${errorMessage}`);

        if (tbody) {
            tbody.innerHTML = `
            <tr>
                <td colspan="7" class="px-6 py-4 whitespace-nowrap text-sm text-red-500 text-center">
                    <i class="fas fa-exclamation-triangle mr-2"></i> 오류: ${errorMessage}
                </td>
            </tr>`;
        }
        if (mobileContainer) {
            mobileContainer.innerHTML = `<div class="text-center py-8 text-red-500"><i class="fas fa-exclamation-triangle mr-2"></i> 오류: ${errorMessage}</div>`;
        }
    }
}

function renderUsers(users) {
    const tbody = document.getElementById('users-table-body');
    const mobileContainer = document.getElementById('mobile-users');

    if (!tbody) return;

    // Clear existing rows
    tbody.innerHTML = '';
    if (mobileContainer) {
        mobileContainer.innerHTML = '';
    }

    if (!Array.isArray(users) || users.length === 0) {
        const tr = document.createElement('tr');
        tr.innerHTML = `
            <td colspan="7" class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-center">
                <i class="fas fa-info-circle mr-2"></i> 표시할 사용자가 없습니다.
            </td>`;
        tbody.appendChild(tr);

        if (mobileContainer) {
            mobileContainer.innerHTML = '<div class="text-center py-8 text-gray-500">표시할 사용자가 없습니다.</div>';
        }
        return;
    }

    users.forEach(user => {
        if (!user) return;

        // Make sure user properties exist before accessing them
        const username = user.username || user.user_name || '이름 없음';
        const userId = user.id || user.user_id || '';
        const email = user.email || '-';
        const userType = user.role || '사용자';
        const isActive = user.is_active !== undefined ? user.is_active : true;
        const lastLogin = user.last_login_at || user.last_login || null;

        const statusText = isActive ? '활성' : '비활성';
        const firstLetter = username ? username.charAt(0).toUpperCase() : 'U';

        // Render desktop table row
        const tr = document.createElement('tr');
        tr.innerHTML = `
        <td class="table-cell">
            <div class="flex items-center">
                <div class="h-10 w-10 flex-shrink-0 bg-gray-100 rounded-full flex items-center justify-center">
                    <span class="text-gray-500 font-medium">${firstLetter}</span>
                </div>
                <div class="ml-4">
                    <div class="font-medium text-gray-900">${username}</div>
                </div>
            </div>
        </td>
        <td class="table-cell-secondary">
            ${email}
        </td>
        <td class="table-cell-secondary text-center">
            <span class="badge badge-info">
                ${roleMap[user.role_id] || userType}
            </span>
        </td>
        <td class="table-cell-secondary text-center">
            <span class="badge ${isActive ? 'badge-success' : 'badge-gray'}">
                ${statusText}
            </span>
        </td>
        <td class="table-cell-secondary text-center">
            ${lastLogin ? new Date(lastLogin).toLocaleString() : '로그인 기록 없음'}
        </td>
        <td class="table-cell text-right">
            <button onclick="editUser(${userId})" class="text-primary-600 hover:text-primary-900">
                <i class="fas fa-edit mr-1"></i>수정
            </button>
        </td>
    `;
        tbody.appendChild(tr);

        // Render mobile card
        if (mobileContainer) {
            const card = document.createElement('div');
            card.className = 'mobile-card';
            card.innerHTML = `
                <div class="flex items-center mb-3 pb-3 border-b border-gray-200">
                    <div class="h-12 w-12 flex-shrink-0 bg-gray-100 rounded-full flex items-center justify-center">
                        <span class="text-gray-500 font-medium text-lg">${firstLetter}</span>
                    </div>
                    <div class="ml-3 flex-1">
                        <div class="font-medium text-gray-900">${username}</div>
                        <div class="text-sm text-gray-500">${email}</div>
                    </div>
                    <span class="badge ${isActive ? 'badge-success' : 'badge-gray'}">
                        ${statusText}
                    </span>
                </div>
                <div class="mobile-card-field">
                    <span class="mobile-card-label">역할:</span>
                    <span class="mobile-card-value">
                        <span class="badge badge-info">${roleMap[user.role_id] || userType}</span>
                    </span>
                </div>
                <div class="mobile-card-field">
                    <span class="mobile-card-label">마지막 로그인:</span>
                    <span class="mobile-card-value">${lastLogin ? new Date(lastLogin).toLocaleString() : '로그인 기록 없음'}</span>
                </div>
                <div class="mt-3 pt-3 border-t border-gray-200 flex justify-end">
                    <button onclick="editUser(${userId})" class="text-primary-600 hover:text-primary-900 font-medium">
                        <i class="fas fa-edit mr-1"></i> 수정
                    </button>
                </div>
            `;
            mobileContainer.appendChild(card);
        }
    });
}

function updatePagination(total, currentPage, itemsPerPage) {
    const totalPages = Math.ceil(total / itemsPerPage);
    const pagination = document.querySelector('.pagination-container');
    if (!pagination) return;

    let paginationHTML = `
    <div class="flex-1 flex justify-between sm:hidden">
        <button onclick="changePage(${currentPage > 1 ? currentPage - 1 : 1})" class="btn btn-secondary">
            이전
        </button>
        <button onclick="changePage(${currentPage < totalPages ? currentPage + 1 : totalPages})" class="btn btn-secondary">
            다음
        </button>
    </div>
    <div class="hidden sm:flex-1 sm:flex sm:items-center sm:justify-between">
        <div>
            <p class="text-sm text-gray-700">
                총 <span class="font-medium">${total}</span>명 중
                <span class="font-medium">${(currentPage - 1) * itemsPerPage + 1}</span> - 
                <span class="font-medium">${Math.min(currentPage * itemsPerPage, total)}</span>명 표시
            </p>
        </div>
        <div>
            <nav class="relative z-0 inline-flex rounded-md shadow-sm -space-x-px" aria-label="Pagination">
                <button onclick="changePage(${currentPage > 1 ? currentPage - 1 : 1})" class="btn btn-secondary btn-sm rounded-r-none">
                    <span class="sr-only">이전</span>
                    <i class="fas fa-chevron-left"></i>
                </button>
`;

    // Generate page numbers
    for (let i = 1; i <= totalPages; i++) {
        if (i === currentPage) {
            paginationHTML += `
            <button aria-current="page" class="z-10 bg-primary-50 border-primary-500 text-primary-600 relative inline-flex items-center px-4 py-2 border text-sm font-medium">
                ${i}
            </button>
        `;
        } else {
            paginationHTML += `
            <button onclick="changePage(${i})" class="bg-white border-gray-300 text-gray-500 hover:bg-gray-50 relative inline-flex items-center px-4 py-2 border text-sm font-medium">
                ${i}
            </button>
        `;
        }
    }

    paginationHTML += `
                <button onclick="changePage(${currentPage < totalPages ? currentPage + 1 : totalPages})" class="btn btn-secondary btn-sm rounded-l-none">
                    <span class="sr-only">다음</span>
                    <i class="fas fa-chevron-right"></i>
                </button>
            </nav>
        </div>
    </div>
`;

    pagination.innerHTML = paginationHTML;
}

function changePage(page) {
    UserList.currentPage = page;
    loadUsers();
}

function showError(message) {
    console.error(message);
    const errorDiv = document.createElement('div');
    errorDiv.className = 'bg-red-50 border-l-4 border-red-500 p-4 mb-4';
    errorDiv.innerHTML = `
        <div class="flex">
            <div class="flex-shrink-0">
                <i class="fas fa-exclamation-circle text-red-500"></i>
            </div>
            <div class="ml-3">
                <p class="text-sm text-red-700">${message}</p>
            </div>
        </div>`;

    // Insert at the top of the content
    const content = document.querySelector('.page-header')?.parentNode;
    if (content) {
        content.insertBefore(errorDiv, content.firstChild);
    }
}

// Export for global access if needed
window.changePage = changePage;

// Modal Functions
async function openUserModal(userId = null) {
    const modal = document.getElementById('userModal');
    const form = document.getElementById('userForm');
    const modalTitle = document.getElementById('modalTitle');
    const submitBtnText = document.getElementById('submit-btn-text');

    if (!modal || !form) return;

    form.reset();
    document.querySelectorAll('[id$="-error"]').forEach(el => el.textContent = '');

    // Clear attributes container
    const container = document.getElementById('attributes-container');
    if (container) container.innerHTML = '';

    // Load user types first
    await loadUserTypes();

    if (userId) {
        modalTitle.textContent = '사용자 수정';
        submitBtnText.textContent = '수정';
        document.getElementById('userId').value = userId;
        document.getElementById('passwordRequired').style.display = 'none';
        document.getElementById('confirmPasswordRequired').style.display = 'none';
        document.getElementById('password').removeAttribute('required');
        document.getElementById('confirmPassword').removeAttribute('required');
        await loadUserData(userId);
    } else {
        modalTitle.textContent = '사용자 추가';
        submitBtnText.textContent = '추가';
        document.getElementById('userId').value = '';
        document.getElementById('passwordRequired').style.display = '';
        document.getElementById('confirmPasswordRequired').style.display = '';
        document.getElementById('password').setAttribute('required', 'required');
        document.getElementById('confirmPassword').setAttribute('required', 'required');
    }

    modal.classList.add('active');
    document.body.style.overflow = 'hidden';
}

function closeUserModal() {
    const modal = document.getElementById('userModal');
    if (modal) {
        modal.classList.remove('active');
    }
    document.body.style.overflow = '';
}

async function loadUserTypes() {
    try {
        const userTypes = await window.apiClient.get('/api/roles');
        const select = document.getElementById('roleId');
        if (!select) return;

        // Clear existing options except the first one
        while (select.options.length > 1) {
            select.remove(1);
        }

        // Handle response format
        let types = [];
        if (Array.isArray(userTypes)) {
            types = userTypes;
        } else if (userTypes && Array.isArray(userTypes.items)) {
            types = userTypes.items;
        } else if (userTypes && userTypes.data && Array.isArray(userTypes.data)) {
            types = userTypes.data;
        }

        types.forEach(userType => {
            const option = document.createElement('option');
            option.value = userType.id;
            option.textContent = userType.name;
            select.appendChild(option);
        });
    } catch (error) {
        console.error('Error loading user types:', error);
        Swal.fire('오류', '사용자 유형을 불러오는 중 오류가 발생했습니다.', 'error');
    }
}

async function loadUserData(userId) {
    try {
        let user = await window.apiClient.get(`/api/users/detail?id=${userId}`);

        // Handle wrapped response if necessary
        if (user.data) user = user.data;
        if (user.user) user = user.user;

        document.getElementById('username').value = user.username || '';
        document.getElementById('username').disabled = true;
        document.getElementById('name').value = user.name || '';
        document.getElementById('email').value = user.email || '';
        document.getElementById('email').disabled = true;
        document.getElementById('phone').value = user.phone || '';
        document.getElementById('roleId').value = user.role_id || '';
        document.getElementById('isActive').checked = user.is_active !== false;
        document.getElementById('forcePasswordChange').checked = user.force_password_change || false;

        // Handle attributes
        const container = document.getElementById('attributes-container');
        if (container) {
            container.innerHTML = ''; // Clear existing
            if (user.attributes) {
                try {
                    const attrObj = typeof user.attributes === 'string' ? JSON.parse(user.attributes) : user.attributes;
                    Object.entries(attrObj).forEach(([key, value]) => {
                        addAttributeRow(key, value);
                    });
                } catch (e) {
                    console.error('Error parsing attributes:', e);
                }
            }
        }
    } catch (error) {
        console.error('Error loading user:', error);
        Swal.fire('오류', '사용자 정보를 불러오는 중 오류가 발생했습니다.', 'error');
    }
}

async function handleUserFormSubmit(e) {
    e.preventDefault();

    const userId = document.getElementById('userId').value;
    const isNewUser = !userId || userId === '';

    const formData = {
        username: document.getElementById('username').value.trim(),
        name: document.getElementById('name').value.trim(),
        email: document.getElementById('email').value.trim(),
        phone: document.getElementById('phone').value.trim(),
        role_id: parseInt(document.getElementById('roleId').value),
        is_active: document.getElementById('isActive').checked,
        force_password_change: document.getElementById('forcePasswordChange').checked
    };

    // Validate and add attributes
    // Validate and add attributes
    const attributeRows = document.querySelectorAll('.attribute-row');
    const attributes = {};
    let hasAttributeError = false;

    attributeRows.forEach(row => {
        const keyInput = row.querySelector('.attr-key');
        const valueInput = row.querySelector('.attr-value');

        const key = keyInput.value.trim();
        let value = valueInput.value.trim();

        if (key) {
            // Try to parse value as number or boolean if possible
            if (value === 'true') value = true;
            else if (value === 'false') value = false;
            else if (!isNaN(value) && value !== '') value = Number(value);

            attributes[key] = value;
        }
    });

    if (Object.keys(attributes).length > 0) {
        formData.attributes = JSON.stringify(attributes);
    } else {
        formData.attributes = null;
    }

    // Password validation for new users
    if (isNewUser) {
        const password = document.getElementById('password').value.trim();
        const confirmPassword = document.getElementById('confirmPassword').value.trim();

        if (!password) {
            Swal.fire('오류', '비밀번호를 입력해주세요.', 'error');
            return;
        }

        if (password.length < 8) {
            Swal.fire('오류', '비밀번호는 8자 이상이어야 합니다.', 'error');
            return;
        }

        if (password !== confirmPassword) {
            Swal.fire('오류', '비밀번호가 일치하지 않습니다.', 'error');
            return;
        }

        formData.password = password;
    } else {
        const password = document.getElementById('password').value.trim();
        if (password) {
            if (password.length < 8) {
                Swal.fire('오류', '비밀번호는 8자 이상이어야 합니다.', 'error');
                return;
            }
            const confirmPassword = document.getElementById('confirmPassword').value.trim();
            if (password !== confirmPassword) {
                Swal.fire('오류', '비밀번호가 일치하지 않습니다.', 'error');
                return;
            }
            formData.password = password;
        }
    }

    try {
        if (isNewUser) {
            await window.apiClient.post('/api/users', formData);
        } else {
            await window.apiClient.put(`/api/users/update?id=${userId}`, formData);
        }

        await Swal.fire({
            title: '성공!',
            text: isNewUser ? '새 사용자가 추가되었습니다.' : '사용자 정보가 수정되었습니다.',
            icon: 'success',
            confirmButtonText: '확인'
        });

        closeUserModal();
        loadUsers();
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

function editUser(userId) {
    openUserModal(userId);
}


// Export functions
// Attribute management functions
function addAttributeRow(key = '', value = '') {
    const container = document.getElementById('attributes-container');
    if (!container) return;

    const rowId = 'attr-' + Date.now() + Math.random().toString(36).substr(2, 9);
    const row = document.createElement('div');
    row.className = 'attribute-row flex gap-2 items-start';
    row.id = rowId;

    row.innerHTML = `
        <div class="flex-1">
            <input type="text" class="attr-key input-field text-sm py-1" placeholder="키 (예: department)" value="${key}">
        </div>
        <div class="flex-1">
            <input type="text" class="attr-value input-field text-sm py-1" placeholder="값 (예: IT)" value="${value}">
        </div>
        <button type="button" onclick="removeAttributeRow('${rowId}')" class="text-red-500 hover:text-red-700 p-2">
            <i class="fas fa-trash-alt"></i>
        </button>
    `;

    container.appendChild(row);
}

function removeAttributeRow(rowId) {
    const row = document.getElementById(rowId);
    if (row) {
        row.remove();
    }
}

// Export functions
window.openUserModal = openUserModal;
window.closeUserModal = closeUserModal;
window.editUser = editUser;
window.addAttributeRow = addAttributeRow;
window.removeAttributeRow = removeAttributeRow;

async function fetchRoles() {
    try {
        const response = await window.apiClient.get('/api/roles');
        const roles = Array.isArray(response) ? response : (response.items || response.data || []);

        roles.forEach(role => {
            roleMap[role.id] = role.name;
        });
    } catch (error) {
        console.error('Error fetching roles:', error);
    }
}
