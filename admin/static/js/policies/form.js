/**
 * Policy Form Script
 */

document.addEventListener('DOMContentLoaded', function () {
    const form = document.getElementById('policy-form');
    const policyDataElement = document.getElementById('policy-data');
    let policyId = '';

    if (policyDataElement && policyDataElement.textContent.trim()) {
        try {
            const policy = JSON.parse(policyDataElement.textContent);
            if (policy && policy.id) {
                policyId = policy.id;
                // Populate form
                const resourceSelect = document.getElementById('resource');
                if (resourceSelect) resourceSelect.value = policy.resource || '';

                const actionSelect = document.getElementById('action');
                if (actionSelect) actionSelect.value = policy.action || '';

                const conditionInput = document.getElementById('condition');
                if (conditionInput) conditionInput.value = policy.condition || '';

                const operatorSelect = document.getElementById('operator');
                if (operatorSelect) operatorSelect.value = policy.operator || '';

                const valueInput = document.getElementById('value');
                if (valueInput) valueInput.value = policy.value || '';

                const descriptionInput = document.getElementById('description');
                if (descriptionInput) descriptionInput.value = policy.description || '';

                const isActiveInput = document.getElementById('is_active');
                if (isActiveInput) isActiveInput.checked = policy.is_active !== false;
            }
        } catch (e) {
            console.error('Error parsing policy data:', e);
        }
    }

    if (form) {
        form.addEventListener('submit', async function (e) {
            e.preventDefault();

            const formData = {
                resource: document.getElementById('resource').value,
                action: document.getElementById('action').value,
                condition: document.getElementById('condition').value,
                operator: document.getElementById('operator').value,
                value: document.getElementById('value').value,
                description: document.getElementById('description').value || null,
                is_active: document.getElementById('is_active').checked
            };

            try {
                if (window.Swal) {
                    Swal.fire({
                        title: '저장 중...',
                        text: '잠시만 기다려주세요.',
                        allowOutsideClick: false,
                        didOpen: () => {
                            Swal.showLoading();
                        }
                    });
                }

                if (policyId) {
                    await window.apiClient.put(`/api/access-control/policies/${policyId}`, formData);
                    if (window.Swal) {
                        await Swal.fire({
                            title: '성공!',
                            text: '정책이 수정되었습니다.',
                            icon: 'success',
                            confirmButtonText: '확인'
                        });
                    }
                } else {
                    await window.apiClient.post('/api/access-control/policies', formData);
                    if (window.Swal) {
                        await Swal.fire({
                            title: '성공!',
                            text: '정책이 생성되었습니다.',
                            icon: 'success',
                            confirmButtonText: '확인'
                        });
                    }
                }

                window.location.href = '/policies';
            } catch (error) {
                console.error('Error saving policy:', error);
                if (window.Swal) {
                    await Swal.fire('오류', '정책 저장 중 오류가 발생했습니다.', 'error');
                } else {
                    alert('오류가 발생했습니다.');
                }
            }
        });
    }
});
