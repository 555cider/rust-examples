/**
 * Policies List Page Script
 */

const modal = new Modal('policyModal');
const table = new DataTable('policies-table-body', {
    itemsPerPage: 10,
    paginationInfoId: 'pagination-info',
    paginationButtonsId: 'pagination-buttons'
});

let currentFilters = {};

// Open modal for new policy
function openPolicyModal() {
    modal.open('정책 추가');
    document.getElementById('submit-btn-text').textContent = '추가';
    document.getElementById('policy-id').value = '';
    document.getElementById('policy-name').value = '';
    document.getElementById('policy-is-active').checked = true;
}

// Open modal for editing
async function editPolicy(id, policy) {
    modal.open('정책 수정');
    document.getElementById('submit-btn-text').textContent = '수정';
    document.getElementById('policy-id').value = id;

    try {
        // Fetch fresh data
        const freshPolicy = await window.apiClient.get(`/api/access-control/policies/detail?id=${id}`);

        document.getElementById('policy-name').value = freshPolicy.name || policy.name || '';
        document.getElementById('policy-resource').value = freshPolicy.resource || policy.resource;
        document.getElementById('policy-action').value = freshPolicy.action || policy.action;
        document.getElementById('policy-condition').value = freshPolicy.condition || policy.condition;
        document.getElementById('policy-operator').value = freshPolicy.operator || policy.operator;
        document.getElementById('policy-value').value = freshPolicy.value || policy.value;
        document.getElementById('policy-description').value = freshPolicy.description || policy.description || '';
        document.getElementById('policy-is-active').checked = freshPolicy.is_active !== undefined ? freshPolicy.is_active : policy.is_active;

        // Parse policy_json and rules
        const pj = freshPolicy.policy_json || {};
        const rules = freshPolicy.rules || [];

        // Map Target
        if (pj.resources && pj.resources.length > 0) document.getElementById('policy-resource').value = pj.resources[0];
        if (pj.actions && pj.actions.length > 0) document.getElementById('policy-action').value = pj.actions[0];

        // Map Rules (Take the first rule for now as the form only supports one)
        if (rules.length > 0) {
            const rule = rules[0];
            // The Rust struct flattens the expression into the condition object
            // So rule.condition IS the expression object (for Simple variant)
            if (rule.condition) {
                // Handle simple condition
                const expr = rule.condition;
                if (expr.attribute) document.getElementById('policy-condition').value = expr.attribute;
                if (expr.operator) document.getElementById('policy-operator').value = expr.operator;
                if (expr.value !== undefined) {
                    // Handle different value types
                    let val = expr.value;
                    if (typeof val === 'object' && val !== null) {
                        // Try to extract value from object wrapper if it exists (e.g. {String: "val"})
                        const keys = Object.keys(val);
                        if (keys.length === 1) val = val[keys[0]];
                    }
                    document.getElementById('policy-value').value = val;
                }
            }
        }

    } catch (error) {
        console.error('Error fetching policy details:', error);
        await showToast('경고', '최신 정보를 불러오지 못해 목록의 데이터를 사용합니다.', 'warning');

        // Fallback to passed data if available (though passed data might be incomplete)
        if (policy) {
            document.getElementById('policy-name').value = policy.name || '';
            document.getElementById('policy-description').value = policy.description || '';
            document.getElementById('policy-is-active').checked = policy.is_active;
        }
    }
}

// Close modal
function closePolicyModal() {
    modal.close();
}

// Load policies
async function loadPolicies() {
    table.showLoading();

    try {
        const search = document.getElementById('search').value;
        const resource = document.getElementById('resource-filter').value;
        const isActive = document.getElementById('status-filter').value;

        const params = {
            page: table.getPage(),
            limit: table.itemsPerPage,
            ...(search && { search }),
            ...(resource && { resource }),
            ...(isActive && { is_active: isActive })
        };

        currentFilters = params;

        const queryString = new URLSearchParams(params).toString();
        const data = await window.apiClient.get(`/api/access-control/policies?${queryString}`);
        const { items, total } = normalizeAPIResponse(data);

        table.render(items, renderPolicyRow);
        table.updatePagination(total);
    } catch (error) {
        console.error('Error loading policies:', error);
        table.showError('정책 목록을 불러오는데 실패했습니다');
        await showToast('오류', '정책 목록을 불러올 수 없습니다', 'error');
    }
}

// Render policy row
function renderPolicyRow(policy) {
    const statusIcon = policy.is_active
        ? '<span class="badge-success"><i class="fas fa-check-circle mr-1"></i>활성</span>'
        : '<span class="badge-gray"><i class="fas fa-times-circle mr-1"></i>비활성</span>';

    // Extract data from policy_json structure
    const resources = policy.policy_json?.resources || [];
    const actions = policy.policy_json?.actions || [];
    const subjects = policy.policy_json?.subjects || [];

    const resourceText = resources.length > 0 ? resources.join(', ') : '-';
    const actionText = actions.length > 0 ? actions.join(', ') : '-';
    const subjectText = subjects.length > 0 ? subjects.join(', ') : '-';

    return `
        <tr class="table-row">
            <td class="table-cell">
                <span class="badge-primary">${resourceText}</span>
            </td>
            <td class="table-cell text-center">
                <span class="badge-info">${actionText}</span>
            </td>
            <td class="table-cell font-mono text-xs" title="${policy.description || ''}">${policy.name || policy.id}</td>
            <td class="table-cell-secondary">${subjectText}</td>
            <td class="table-cell text-center">${policy.priority || '-'}</td>
            <td class="table-cell text-center">${statusIcon}</td>
            <td class="table-cell-secondary text-center">${formatDate(policy.created_at)}</td>
            <td class="table-cell text-right">
                <button onclick='editPolicy("${policy.id}", ${JSON.stringify(policy).replace(/'/g, "&apos;")})'
                        class="text-primary-600 hover:text-primary-900 mr-3" title="수정">
                    <i class="fas fa-edit mr-1"></i>수정
                </button>
                <button onclick='togglePolicy("${policy.id}", ${policy.is_active})'
                        class="text-warning-600 hover:text-warning-900 mr-3"
                        title="${policy.is_active ? '비활성화' : '활성화'}">
                    <i class="fas fa-power-off mr-1"></i>${policy.is_active ? '끄기' : '켜기'}
                </button>
                <button onclick='deletePolicy("${policy.id}")'
                        class="text-error-600 hover:text-error-900" title="삭제">
                    <i class="fas fa-trash mr-1"></i>삭제
                </button>
            </td>
        </tr>
    `;
}

// Toggle policy status
async function togglePolicy(id, currentStatus) {
    try {
        await window.apiClient.post(`/api/access-control/policies/toggle?id=${id}`, {});
        await showToast('성공', '정책 상태가 변경되었습니다', 'success');
        loadPolicies();
    } catch (error) {
        console.error('Error toggling policy:', error);
        await showToast('오류', '정책 상태 변경에 실패했습니다', 'error');
    }
}

// Delete policy
async function deletePolicy(id) {
    const result = await showConfirm(
        '정책 삭제',
        '정말 이 정책을 삭제하시겠습니까? 이 작업은 되돌릴 수 없습니다.',
        '삭제',
        '취소'
    );

    if (!result.isConfirmed) return;

    try {
        await window.apiClient.delete(`/api/access-control/policies/delete?id=${id}`);
        await showToast('성공', '정책이 삭제되었습니다', 'success');
        loadPolicies();
    } catch (error) {
        console.error('Error deleting policy:', error);
        await showToast('오류', '정책 삭제에 실패했습니다', 'error');
    }
}

// Pagination
function changePage(page) {
    table.setPage(page);
    loadPolicies();
}

// Initialize
document.addEventListener('DOMContentLoaded', function () {
    // Submit form
    document.getElementById('policyModalForm').addEventListener('submit', async function (e) {
        e.preventDefault();

        const policyId = document.getElementById('policy-id').value;
        const policyName = document.getElementById('policy-name').value;

        // Construct Policy JSON structure
        const resource = document.getElementById('policy-resource').value;
        const action = document.getElementById('policy-action').value;
        const conditionAttr = document.getElementById('policy-condition').value;
        const operator = document.getElementById('policy-operator').value;
        let value = document.getElementById('policy-value').value;

        // Try to parse value if it looks like a number or boolean
        if (value === 'true') value = true;
        else if (value === 'false') value = false;
        else if (!isNaN(value) && value !== '') value = Number(value);

        const policyJson = {
            id: policyId || 'policy-' + Date.now(), // Generate ID if new
            name: policyName,
            effect: 'PERMIT',
            subjects: [], // Default to all for now or add field
            resources: [resource],
            actions: [action],
            rules: [
                {
                    effect: 'PERMIT',
                    condition: {
                        attribute: conditionAttr,
                        operator: operator,
                        value: value
                    }
                }
            ]
        };

        // Since the backend expects a flattened structure for creation/update in some handlers 
        // but creates a complex structure internally, we need to match what `CreatePolicyRequest` expects.
        // `CreatePolicyRequest` expects `policy_json` as a STRING.

        const requestData = {
            id: policyId || 'policy-' + Date.now(),
            name: policyName,
            description: document.getElementById('policy-description').value || null,
            policy_json: JSON.stringify(policyJson),
            priority: 0,
            is_active: document.getElementById('policy-is-active').checked
        };

        try {
            if (policyId) {
                // Update expects UpdatePolicyRequest which has optional fields
                const updateData = {
                    name: requestData.name,
                    description: requestData.description,
                    policy_json: requestData.policy_json,
                    is_active: requestData.is_active
                };
                await window.apiClient.put(`/api/access-control/policies/update?id=${policyId}`, updateData);
                await showToast('성공', '정책이 수정되었습니다', 'success');
            } else {
                await window.apiClient.post('/api/access-control/policies', requestData);
                await showToast('성공', '정책이 추가되었습니다', 'success');
            }

            closePolicyModal();
            loadPolicies();
        } catch (error) {
            console.error('Error saving policy:', error);

            if (error.response && error.response.data && error.response.data.errors) {
                modal.showFieldErrors(error.response.data.errors);
            } else {
                await showToast('오류', '정책 저장에 실패했습니다: ' + (error.response?.data?.message || error.message), 'error');
            }
        }
    });

    // Search button
    document.getElementById('search-button').addEventListener('click', () => {
        table.setPage(1);
        loadPolicies();
    });

    // Search on Enter key
    document.getElementById('search').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            table.setPage(1);
            loadPolicies();
        }
    });

    // Filter changes
    document.getElementById('resource-filter').addEventListener('change', () => {
        table.setPage(1);
        loadPolicies();
    });

    document.getElementById('status-filter').addEventListener('change', () => {
        table.setPage(1);
        loadPolicies();
    });

    // Initial load
    loadPolicies();
});

// Export functions for global access
window.openPolicyModal = openPolicyModal;
window.editPolicy = editPolicy;
window.closePolicyModal = closePolicyModal;
window.togglePolicy = togglePolicy;
window.deletePolicy = deletePolicy;
window.changePage = changePage;
