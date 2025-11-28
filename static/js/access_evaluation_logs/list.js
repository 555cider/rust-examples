/**
 * Access Logs List Script
 */

let modal;
let table;

document.addEventListener('DOMContentLoaded', () => {
    modal = new Modal('logDetailModal');
    table = new DataTable('access-logs-table-body', {
        itemsPerPage: 20,
        paginationInfoId: 'pagination-info',
        paginationButtonsId: 'pagination-buttons'
    });

    // Initial load
    loadLogs();

    // Event Listeners
    document.getElementById('search-button')?.addEventListener('click', () => {
        table.setPage(1);
        loadLogs();
    });

    document.getElementById('search')?.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
            table.setPage(1);
            loadLogs();
        }
    });

    document.getElementById('result-filter')?.addEventListener('change', () => {
        table.setPage(1);
        loadLogs();
    });

    document.getElementById('evaluation-type-filter')?.addEventListener('change', () => {
        table.setPage(1);
        loadLogs();
    });
});

// Helper functions for evaluation type
function getEvaluationType(log) {
    if (log.rbac_result && log.abac_result) {
        return 'hybrid';
    } else if (log.rbac_result) {
        return 'rbac';
    } else if (log.abac_result) {
        return 'abac';
    }
    return 'unknown';
}

function getEvaluationTypeLabel(type) {
    const labels = {
        'hybrid': '하이브리드',
        'rbac': 'RBAC',
        'abac': 'ABAC',
        'unknown': '알 수 없음'
    };
    return labels[type] || type;
}

function getEvaluationTypeBadge(type) {
    const badges = {
        'hybrid': '<span class="badge badge-primary"><i class="fas fa-layer-group mr-1"></i>하이브리드</span>',
        'rbac': '<span class="badge badge-info"><i class="fas fa-user-shield mr-1"></i>RBAC</span>',
        'abac': '<span class="badge badge-success"><i class="fas fa-shield-alt mr-1"></i>ABAC</span>',
        'unknown': '<span class="badge badge-gray">알 수 없음</span>'
    };
    return badges[type] || badges.unknown;
}

function getDecisionBadge(decision) {
    const badges = {
        'Permit': '<span class="badge badge-success"><i class="fas fa-check-circle mr-1"></i>허용</span>',
        'Deny': '<span class="badge badge-error"><i class="fas fa-times-circle mr-1"></i>거부</span>',
        'NotApplicable': '<span class="badge badge-gray"><i class="fas fa-minus-circle mr-1"></i>해당없음</span>'
    };
    return badges[decision] || badges.NotApplicable;
}

// Load logs
async function loadLogs() {
    if (!table) return;
    table.showLoading();

    try {
        const search = document.getElementById('search').value;
        const result = document.getElementById('result-filter').value;
        const evaluationType = document.getElementById('evaluation-type-filter').value;

        const params = {
            page: table.getPage(),
            limit: table.itemsPerPage,
            ...(search && { search }),
            ...(result && { result: result }),
            ...(evaluationType && { evaluation_type: evaluationType })
        };

        let data;
        if (window.apiClient) {
            const queryString = new URLSearchParams(params).toString();
            data = await window.apiClient.get(`/api/access-control/logs?${queryString}`);
        } else {
            // Fallback if apiClient is not available (should not happen)
            const queryString = new URLSearchParams(params).toString();
            const response = await fetch(`/api/access-control/logs?${queryString}`);
            if (!response.ok) throw new Error('Failed to fetch logs');
            data = await response.json();
        }

        // Normalize response
        let items = [];
        let total = 0;

        if (data.logs) {
            // Primary response format from AccessEvaluationLogListResponse
            items = data.logs;
            total = data.total_count || 0;
        } else if (data.items) {
            items = data.items;
            total = data.total || 0;
        } else if (Array.isArray(data)) {
            items = data;
            total = data.length;
        } else if (data.data && Array.isArray(data.data)) {
            items = data.data;
            total = data.total || data.data.length;
        } else if (data.data && data.data.items) {
            items = data.data.items;
            total = data.data.total || 0;
        }

        table.render(items, renderLogRow);
        table.updatePagination(total);
    } catch (error) {
        console.error('Error loading logs:', error);
        table.showError('로그를 불러오는데 실패했습니다');

        if (window.Swal) {
            await Swal.fire({
                title: '오류',
                text: '로그 목록을 불러올 수 없습니다',
                icon: 'error',
                confirmButtonText: '확인'
            });
        }
    }
}

// Render log row
function renderLogRow(log) {
    const evaluationType = getEvaluationType(log);
    const evaluationBadge = getEvaluationTypeBadge(evaluationType);
    const decisionBadge = getDecisionBadge(log.decision);

    // Format date using utility if available, otherwise simple format
    const dateStr = window.formatDate ? window.formatDate(log.created_at, true) : new Date(log.created_at).toLocaleString();

    return `
        <tr class="hover:bg-gray-50 transition-colors cursor-pointer" onclick="viewLogDetail(${log.id})">
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-center">
                ${dateStr}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900">
                ${log.user_id ? `User #${log.user_id}` : '<span class="text-gray-400">N/A</span>'}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900">
                <div class="flex flex-col">
                    <span class="font-medium">${log.resource_type || '<span class="text-gray-400">N/A</span>'}</span>
                    ${log.resource_id ? `<span class="text-xs text-gray-500 font-mono">${log.resource_id}</span>` : ''}
                </div>
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 text-center">
                <span class="px-2 py-1 rounded-md bg-gray-100 text-gray-700 text-xs font-medium font-mono">${log.action}</span>
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-center">
                ${decisionBadge}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-center">
                ${evaluationBadge}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500 text-right">
                ${log.evaluation_time_ms ? `${log.evaluation_time_ms}ms` : '<span class="text-gray-400">N/A</span>'}
            </td>
            <td class="px-6 py-4 whitespace-nowrap text-right text-sm font-medium">
                <button onclick="event.stopPropagation(); viewLogDetail(${log.id})"
                        class="text-primary-600 hover:text-primary-900 transition-colors p-2 rounded-full hover:bg-primary-50" title="상세보기">
                    <i class="fas fa-eye"></i>
                </button>
            </td>
        </tr>
    `;
}

// View log detail
window.viewLogDetail = async function (logId) {
    try {
        let log;
        if (window.apiClient) {
            log = await window.apiClient.get(`/api/access-control/logs/detail?id=${logId}`);
        } else {
            const response = await fetch(`/api/access-control/logs/detail?id=${logId}`);
            if (!response.ok) throw new Error('Failed to fetch log detail');
            log = await response.json();
        }

        const evaluationType = getEvaluationType(log);
        const evaluationBadge = getEvaluationTypeBadge(evaluationType);
        const decisionBadge = getDecisionBadge(log.decision);
        const dateStr = window.formatDate ? window.formatDate(log.created_at, true) : new Date(log.created_at).toLocaleString();

        // Defensive parsing for context and applied_policies
        if (typeof log.context === 'string') {
            try {
                log.context = JSON.parse(log.context);
            } catch (e) {
                console.error('Failed to parse context JSON:', e);
            }
        }

        if (typeof log.applied_policies === 'string') {
            try {
                log.applied_policies = JSON.parse(log.applied_policies);
            } catch (e) {
                console.error('Failed to parse applied_policies JSON:', e);
                log.applied_policies = [];
            }
        }

        let html = `
            <div class="space-y-6">
                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div class="bg-gray-50 p-4 rounded-xl border border-gray-100">
                        <label class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-1">Request ID</label>
                        <p class="text-sm text-gray-900 font-mono break-all">${log.request_id}</p>
                    </div>
                    <div class="bg-gray-50 p-4 rounded-xl border border-gray-100">
                        <label class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-1">시간</label>
                        <p class="text-sm text-gray-900">${dateStr}</p>
                    </div>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label class="block text-sm font-medium text-gray-700 mb-1">사용자 ID</label>
                        <div class="p-3 bg-white border border-gray-200 rounded-lg text-sm text-gray-900">
                            ${log.user_id || 'N/A'}
                        </div>
                    </div>
                    <div>
                        <label class="block text-sm font-medium text-gray-700 mb-1">평가 시간</label>
                        <div class="p-3 bg-white border border-gray-200 rounded-lg text-sm text-gray-900">
                            ${log.evaluation_time_ms ? `${log.evaluation_time_ms}ms` : 'N/A'}
                        </div>
                    </div>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                    <div>
                        <label class="block text-sm font-medium text-gray-700 mb-1">리소스 타입</label>
                        <div class="p-3 bg-white border border-gray-200 rounded-lg text-sm text-gray-900">
                            ${log.resource_type || 'N/A'}
                        </div>
                    </div>
                    <div>
                        <label class="block text-sm font-medium text-gray-700 mb-1">리소스 ID</label>
                        <div class="p-3 bg-white border border-gray-200 rounded-lg text-sm text-gray-900 font-mono">
                            ${log.resource_id || 'N/A'}
                        </div>
                    </div>
                </div>

                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">액션</label>
                    <div class="p-3 bg-white border border-gray-200 rounded-lg text-sm text-gray-900">
                        ${log.action}
                    </div>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div class="bg-gray-50 p-4 rounded-xl border border-gray-100 text-center">
                        <label class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">최종 결과</label>
                        <div>${decisionBadge}</div>
                    </div>
                    <div class="bg-gray-50 p-4 rounded-xl border border-gray-100 text-center">
                        <label class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">RBAC 결과</label>
                        <div>${log.rbac_result ? getDecisionBadge(log.rbac_result) : '<span class="text-gray-400 text-sm">N/A</span>'}</div>
                    </div>
                    <div class="bg-gray-50 p-4 rounded-xl border border-gray-100 text-center">
                        <label class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">ABAC 결과</label>
                        <div>${log.abac_result ? getDecisionBadge(log.abac_result) : '<span class="text-gray-400 text-sm">N/A</span>'}</div>
                    </div>
                </div>

                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-1">평가 방식</label>
                    <div class="mt-1">${evaluationBadge}</div>
                </div>

                ${log.applied_policies && log.applied_policies.length > 0 ? `
                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-2">적용된 정책</label>
                    <div class="bg-gray-50 rounded-xl p-4 border border-gray-100">
                        <ul class="list-disc list-inside space-y-1">
                            ${log.applied_policies.map(policy => `<li class="text-sm text-gray-900 font-mono">${policy}</li>`).join('')}
                        </ul>
                    </div>
                </div>
                ` : ''}

                ${log.context ? `
                <div>
                    <label class="block text-sm font-medium text-gray-700 mb-2">컨텍스트</label>
                    <div class="bg-gray-900 rounded-xl p-4 overflow-hidden">
                        <pre class="text-xs text-gray-100 overflow-x-auto font-mono custom-scrollbar">${JSON.stringify(log.context, null, 2)}</pre>
                    </div>
                </div>
                ` : ''}
            </div>
        `;

        document.getElementById('log-detail-content').innerHTML = html;
        if (modal) modal.open('로그 상세 정보');
    } catch (error) {
        console.error('Error loading log detail:', error);
        if (window.Swal) {
            await Swal.fire('오류', '로그 상세 정보를 불러올 수 없습니다', 'error');
        }
    }
};

// Close modal
window.closeLogDetailModal = function () {
    if (modal) modal.close();
};

// Pagination
window.changePage = function (page) {
    if (table) {
        table.setPage(page);
        loadLogs();
        window.scrollTo({ top: 0, behavior: 'smooth' });
    }
};
