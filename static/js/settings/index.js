/**
 * Settings Page Script
 */

document.addEventListener('DOMContentLoaded', function () {
    initializeTabs();
    initializeProfileForm();
});

function initializeTabs() {
    const tabs = document.querySelectorAll('[data-tab]');
    const tabContents = document.querySelectorAll('[data-tab-content]');

    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            const targetTab = tab.getAttribute('data-tab');

            // Update active tab
            tabs.forEach(t => t.classList.remove('border-primary-500', 'text-primary-600'));
            tab.classList.add('border-primary-500', 'text-primary-600');

            // Show target tab content
            tabContents.forEach(content => {
                content.classList.add('hidden');
                if (content.id === `${targetTab}-content`) {
                    content.classList.remove('hidden');
                }
            });
        });
    });
}

function initializeProfileForm() {
    const form = document.getElementById('profile-form');
    if (!form) return;

    form.addEventListener('submit', async function (e) {
        e.preventDefault();

        const currentPassword = document.getElementById('current_password')?.value.trim();
        const newPassword = document.getElementById('new_password')?.value.trim();
        const confirmPassword = document.getElementById('confirm_password')?.value.trim();
        const email = document.getElementById('email')?.value.trim();

        // Validate password change if any password field is filled
        if (currentPassword || newPassword || confirmPassword) {
            if (!currentPassword) {
                showError('현재 비밀번호를 입력해주세요.');
                return;
            }

            if (!newPassword) {
                showError('새 비밀번호를 입력해주세요.');
                return;
            }

            if (newPassword.length < 8) {
                showError('새 비밀번호는 8자 이상이어야 합니다.');
                return;
            }

            if (newPassword !== confirmPassword) {
                showError('새 비밀번호가 일치하지 않습니다.');
                return;
            }
        }

        // Prepare form data
        const formData = {};

        if (email) {
            formData.email = email;
        }

        if (currentPassword && newPassword) {
            formData.current_password = currentPassword;
            formData.new_password = newPassword;
        }

        // If no changes, show message
        if (Object.keys(formData).length === 0) {
            showInfo('변경할 내용이 없습니다.');
            return;
        }

        try {
            let response;
            if (window.apiClient) {
                response = await window.apiClient.put('/api/auth/update-profile', formData);
            } else {
                const res = await fetch('/api/auth/update-profile', {
                    method: 'PUT',
                    headers: {
                        'Content-Type': 'application/json',
                        'Accept': 'application/json',
                        'X-Requested-With': 'XMLHttpRequest'
                    },
                    body: JSON.stringify(formData),
                    credentials: 'same-origin'
                });

                if (!res.ok) {
                    const data = await res.json();
                    throw new Error(data.message || '프로필 업데이트에 실패했습니다.');
                }
                response = await res.json();
            }

            showSuccess('프로필이 성공적으로 업데이트되었습니다.');

            // Clear password fields
            if (document.getElementById('current_password')) {
                document.getElementById('current_password').value = '';
            }
            if (document.getElementById('new_password')) {
                document.getElementById('new_password').value = '';
            }
            if (document.getElementById('confirm_password')) {
                document.getElementById('confirm_password').value = '';
            }

        } catch (error) {
            console.error('Error updating profile:', error);
            showError(error.message || '프로필 업데이트 중 오류가 발생했습니다.');
        }
    });
}

function showError(message) {
    if (window.Swal) {
        Swal.fire({
            title: '오류',
            text: message,
            icon: 'error',
            confirmButtonText: '확인'
        });
    } else {
        alert(message);
    }
}

function showSuccess(message) {
    if (window.Swal) {
        Swal.fire({
            title: '성공',
            text: message,
            icon: 'success',
            confirmButtonText: '확인'
        });
    } else {
        alert(message);
    }
}

function showInfo(message) {
    if (window.Swal) {
        Swal.fire({
            title: '알림',
            text: message,
            icon: 'info',
            confirmButtonText: '확인'
        });
    } else {
        alert(message);
    }
}
