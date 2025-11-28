/**
 * Role List Page Script
 */

document.addEventListener('DOMContentLoaded', function () {
    initializeModal();
});

function initializeModal() {
    const form = document.getElementById('roleForm');
    if (form) {
        form.addEventListener('submit', handleRoleFormSubmit);
    }

    document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape') {
            closeRoleModal();
        }
    });
}

// Modal Functions
async function openRoleModal(roleId = null) {
    const modal = document.getElementById('roleModal');
    const form = document.getElementById('roleForm');
    const modalTitle = document.getElementById('modalTitle');
    const submitBtnText = document.getElementById('submit-btn-text');

    if (!modal || !form) return;

    form.reset();
    document.querySelectorAll('[id$="-error"]').forEach(el => el.textContent = '');

    // Load permissions first
    await loadPermissionsList();

    if (roleId) {
        modalTitle.textContent = '역할 수정';
        submitBtnText.textContent = '수정';
        document.getElementById('roleId').value = roleId;
        await loadRoleData(roleId);
    } else {
        modalTitle.textContent = '역할 추가';
        submitBtnText.textContent = '추가';
        document.getElementById('roleId').value = '';
    }

    modal.classList.add('active');
    document.body.style.overflow = 'hidden';
}

function closeRoleModal() {
    const modal = document.getElementById('roleModal');
    if (modal) {
        modal.classList.remove('active');
    }
    document.body.style.overflow = '';
}

async function loadPermissionsList() {
    const container = document.getElementById('permissionsList');
    container.innerHTML = '<div class="text-center text-gray-500 py-4"><i class="fas fa-spinner fa-spin mr-2"></i> 권한 목록을 불러오는 중...</div>';

    try {
        const response = await window.apiClient.get('/api/permissions?limit=1000'); // Fetch all permissions
        const permissions = Array.isArray(response) ? response : (response.items || response.data || []);

        if (!permissions || permissions.length === 0) {
            container.innerHTML = '<div class="text-center text-gray-500 py-4">등록된 권한이 없습니다.</div>';
            return;
        }

        // Group permissions by category (assuming category exists, otherwise use 'General')
        const grouped = permissions.reduce((acc, perm) => {
            const category = perm.category || '기타';
            if (!acc[category]) acc[category] = [];
            acc[category].push(perm);
            return acc;
        }, {});

        let html = '';
        for (const [category, perms] of Object.entries(grouped)) {
            html += `
                <div class="mb-4">
                    <h4 class="text-sm font-semibold text-gray-700 mb-2 border-b pb-1">${category}</h4>
                    <div class="space-y-2">
            `;

            perms.forEach(perm => {
                html += `
                    <div class="flex items-start">
                        <div class="flex items-center h-5">
                            <input id="perm-${perm.id}" name="permissions" type="checkbox" value="${perm.id}" 
                                class="checkbox-field h-4 w-4 text-primary-600 focus:ring-primary-500 border-gray-300 rounded">
                        </div>
                        <div class="ml-3 text-sm">
                            <label for="perm-${perm.id}" class="font-medium text-gray-700 cursor-pointer">${perm.name}</label>
                            <p class="text-gray-500 text-xs">${perm.code}</p>
                        </div>
                    </div>
                `;
            });

            html += `
                    </div>
                </div>
            `;
        }

        container.innerHTML = html;

    } catch (error) {
        console.error('Error loading permissions:', error);
        container.innerHTML = '<div class="text-center text-red-500 py-4">권한 목록을 불러오는데 실패했습니다.</div>';
    }
}

async function loadRoleData(roleId) {
    try {
        let userType = await window.apiClient.get(`/api/roles/detail?id=${roleId}`);

        // Handle wrapped response if necessary
        if (userType.data) userType = userType.data;
        if (userType.item) userType = userType.item;

        document.getElementById('code').value = userType.code || '';
        document.getElementById('name').value = userType.name || '';
        document.getElementById('description').value = userType.description || '';
        document.getElementById('isActive').checked = userType.is_active !== false;

        // Check assigned permissions
        if (userType.permissions && Array.isArray(userType.permissions)) {
            userType.permissions.forEach(permId => {
                const checkbox = document.getElementById(`perm-${permId}`);
                if (checkbox) checkbox.checked = true;
            });
        }

    } catch (error) {
        console.error('Error loading user type:', error);
        Swal.fire('오류', '역할 정보를 불러오는 중 오류가 발생했습니다.', 'error');
    }
}

async function handleRoleFormSubmit(e) {
    e.preventDefault();

    const roleId = document.getElementById('roleId').value;
    const isNew = !roleId || roleId === '';

    // Get selected permissions
    const selectedPermissions = Array.from(document.querySelectorAll('input[name="permissions"]:checked'))
        .map(cb => parseInt(cb.value));

    const formData = {
        code: document.getElementById('code').value.trim(),
        name: document.getElementById('name').value.trim(),
        description: document.getElementById('description').value.trim(),
        is_active: document.getElementById('isActive').checked,
        permissions: selectedPermissions
    };

    try {
        if (isNew) {
            await window.apiClient.post('/api/roles', formData);
        } else {
            await window.apiClient.put(`/api/roles/update?id=${roleId}`, formData);
        }

        await Swal.fire({
            title: '성공!',
            text: isNew ? '새 역할이 추가되었습니다.' : '역할 정보가 수정되었습니다.',
            icon: 'success',
            confirmButtonText: '확인'
        });

        closeRoleModal();
        // Reload the page to refresh the list
        window.location.reload();
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

function editRole(roleId) {
    openRoleModal(roleId);
}

async function deleteRole(roleId) {
    const result = await Swal.fire({
        title: '역할 삭제',
        text: "정말로 이 역할을 삭제하시겠습니까? 이 작업은 되돌릴 수 없습니다.",
        icon: 'warning',
        showCancelButton: true,
        confirmButtonColor: '#d33',
        cancelButtonColor: '#3085d6',
        confirmButtonText: '삭제',
        cancelButtonText: '취소'
    });

    if (result.isConfirmed) {
        try {
            await window.apiClient.delete(`/api/roles/delete?id=${roleId}`);

            await Swal.fire(
                '삭제됨!',
                '역할이 성공적으로 삭제되었습니다.',
                'success'
            );

            // Reload the page to refresh the list
            window.location.reload();
        } catch (error) {
            console.error('Error deleting role:', error);
            Swal.fire(
                '오류',
                error.message || '역할 삭제 중 오류가 발생했습니다.',
                'error'
            );
        }
    }
}

// Export functions
window.openRoleModal = openRoleModal;
window.closeRoleModal = closeRoleModal;
window.editRole = editRole;
window.deleteRole = deleteRole;
