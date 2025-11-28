/**
 * Access Control Simulator Script
 */

document.addEventListener('DOMContentLoaded', function () {
    loadSubjects();

    // Add initial context field
    addContextField();

    document.getElementById('simulatorForm').addEventListener('submit', handleSimulation);
    document.getElementById('subject').addEventListener('change', handleSubjectChange);
});

async function loadSubjects() {
    try {
        const response = await window.apiClient.get('/api/users?limit=100');
        const users = Array.isArray(response) ? response : (response.items || response.data?.items || response.data || []);
        const select = document.getElementById('subject');

        users.forEach(user => {
            const option = document.createElement('option');
            option.value = user.id;
            option.textContent = `${user.username} (${user.email || 'No email'})`;
            option.dataset.role = user.role_id || 'Unknown';
            select.appendChild(option);
        });
    } catch (error) {
        console.error('Error loading subjects:', error);
        Swal.fire('오류', '사용자 목록을 불러오는데 실패했습니다.', 'error');
    }
}

function handleSubjectChange(e) {
    const selectedOption = e.target.options[e.target.selectedIndex];
    const roleDisplay = document.getElementById('selectedUserRole');

    if (selectedOption.value) {
        roleDisplay.textContent = `현재 역할: ${selectedOption.dataset.role}`;
    } else {
        roleDisplay.textContent = '-';
    }
}

function addContextField() {
    const container = document.getElementById('contextContainer');
    const div = document.createElement('div');
    div.className = 'flex gap-2';
    div.innerHTML = `
        <input type="text" placeholder="Key (e.g., time)" class="input-field w-1/3 context-key">
        <input type="text" placeholder="Value (e.g., 14:00)" class="input-field w-1/3 context-value">
        <select class="input-field w-1/3 context-type">
            <option value="string">String</option>
            <option value="number">Number</option>
            <option value="boolean">Boolean</option>
        </select>
        <button type="button" onclick="this.parentElement.remove()" class="text-red-500 hover:text-red-700 px-2">
            <i class="fas fa-times"></i>
        </button>
    `;
    container.appendChild(div);
}

async function handleSimulation(e) {
    e.preventDefault();

    const subjectId = document.getElementById('subject').value;
    const resource = document.getElementById('resource').value;
    const action = document.getElementById('action').value;

    // Build context object
    const context = {};
    document.querySelectorAll('#contextContainer > div').forEach(div => {
        const key = div.querySelector('.context-key').value.trim();
        const valueStr = div.querySelector('.context-value').value.trim();
        const type = div.querySelector('.context-type').value;

        if (key && valueStr) {
            let value = valueStr;
            if (type === 'number') value = Number(valueStr);
            if (type === 'boolean') value = (valueStr.toLowerCase() === 'true');
            context[key] = value;
        }
    });

    const requestData = {
        subject: subjectId, // The API expects subject as string (user_id)
        resource: resource,
        action: action,
        context: context
    };

    try {
        // Show loading state
        const btn = e.target.querySelector('button[type="submit"]');
        const originalText = btn.innerHTML;
        btn.innerHTML = '<i class="fas fa-spinner fa-spin mr-2"></i> 실행 중...';
        btn.disabled = true;

        const response = await window.apiClient.post('/api/access-control/evaluation/evaluate', requestData);

        displayResults(response);

        btn.innerHTML = originalText;
        btn.disabled = false;
    } catch (error) {
        console.error('Simulation error:', error);
        Swal.fire('오류', '시뮬레이션 실행 중 오류가 발생했습니다.', 'error');

        const btn = e.target.querySelector('button[type="submit"]');
        btn.innerHTML = '<i class="fas fa-play mr-2"></i> 시뮬레이션 실행';
        btn.disabled = false;
    }
}

function displayResults(data) {
    document.getElementById('emptyState').classList.add('hidden');
    document.getElementById('resultContainer').classList.remove('hidden');

    // Overall Result
    const resultCard = document.getElementById('resultCard');
    const resultIcon = document.getElementById('resultIcon');
    const resultText = document.getElementById('resultText');
    const resultTime = document.getElementById('resultTime');

    if (data.granted) {
        resultCard.className = 'glass-card rounded-2xl shadow-xl p-6 text-center border-l-4 border-green-500';
        resultIcon.innerHTML = '<i class="fas fa-check-circle text-green-500"></i>';
        resultText.textContent = 'PERMIT';
        resultText.className = 'text-3xl font-bold text-green-600';
    } else {
        resultCard.className = 'glass-card rounded-2xl shadow-xl p-6 text-center border-l-4 border-red-500';
        resultIcon.innerHTML = '<i class="fas fa-times-circle text-red-500"></i>';
        resultText.textContent = 'DENY';
        resultText.className = 'text-3xl font-bold text-red-600';
    }
    resultTime.textContent = `평가 시간: ${data.evaluation_time_ms}ms`;

    // Reset Steps
    resetSteps();

    // Analyze Decision Path (This logic mimics the HybridEvaluator logic for visualization)
    const rbacResult = data.decision.rbac_result;
    const abacResult = data.decision.abac_result;

    // Step 1: ABAC Deny Check
    // Check for applied ABAC Deny policies
    const abacDenyPolicies = data.applied_policies.filter(p => p.policy_type === 'ABAC' && p.effect === 'Deny');

    if (abacDenyPolicies.length > 0) {
        updateStep('step1', 'deny', `거부 정책 적용됨: ${abacDenyPolicies.map(p => p.policy_name).join(', ')}`);
        updateStep('step2', 'skip', 'ABAC 거부로 인해 건너뜀');
        updateStep('step3', 'skip', 'ABAC 거부로 인해 건너뜀');
    } else {
        updateStep('step1', 'pass', '명시적 거부 정책 없음');

        // Step 2: RBAC Check
        if (rbacResult) {
            if (rbacResult.granted) {
                updateStep('step2', 'pass', `권한 확인됨: ${rbacResult.applied_permissions.join(', ')}`);
                updateStep('step3', 'skip', 'RBAC 허용으로 인해 건너뜀');
            } else {
                updateStep('step2', 'deny', '일치하는 권한 없음');

                // Step 3: ABAC Permit Check
                // Check for ABAC Permit policies
                const abacPermitPolicies = data.applied_policies.filter(p => p.policy_type === 'ABAC' && p.effect === 'Permit');

                if (abacPermitPolicies.length > 0) {
                    updateStep('step3', 'pass', `허용 정책 적용됨: ${abacPermitPolicies.map(p => p.policy_name).join(', ')}`);
                } else {
                    updateStep('step3', 'deny', '명시적 허용 정책 없음');
                }
            }
        } else {
            updateStep('step2', 'skip', 'RBAC 결과 없음');
            updateStep('step3', 'skip', 'RBAC 결과 없음');
        }
    }

    // JSON Response
    document.getElementById('jsonResponse').textContent = JSON.stringify(data, null, 2);
}

function resetSteps() {
    ['step1', 'step2', 'step3'].forEach(step => {
        const icon = document.getElementById(`${step}-icon`);
        const status = document.getElementById(`${step}-status`);
        const details = document.getElementById(`${step}-details`);

        icon.className = 'h-8 w-8 rounded-full bg-gray-100 border-2 border-gray-300 flex items-center justify-center z-10';
        icon.innerHTML = `<span class="text-sm font-bold text-gray-500">${step.replace('step', '')}</span>`;

        status.className = 'badge badge-gray';
        status.textContent = '대기중';

        details.textContent = '';
        details.classList.add('hidden');
    });
}

function updateStep(stepId, result, message) {
    const icon = document.getElementById(`${stepId}-icon`);
    const status = document.getElementById(`${stepId}-status`);
    const details = document.getElementById(`${stepId}-details`);

    details.textContent = message;
    details.classList.remove('hidden');

    if (result === 'pass') {
        icon.className = 'h-8 w-8 rounded-full bg-green-100 border-2 border-green-500 flex items-center justify-center z-10';
        icon.innerHTML = '<i class="fas fa-check text-green-600"></i>';
        status.className = 'badge badge-success';
        status.textContent = '통과';
    } else if (result === 'deny') {
        icon.className = 'h-8 w-8 rounded-full bg-red-100 border-2 border-red-500 flex items-center justify-center z-10';
        icon.innerHTML = '<i class="fas fa-times text-red-600"></i>';
        status.className = 'badge badge-error';
        status.textContent = '거부';
    } else {
        icon.className = 'h-8 w-8 rounded-full bg-gray-100 border-2 border-gray-300 flex items-center justify-center z-10';
        icon.innerHTML = '<i class="fas fa-minus text-gray-400"></i>';
        status.className = 'badge badge-gray';
        status.textContent = '건너뜀';
    }
}

function copyJson() {
    const json = document.getElementById('jsonResponse').textContent;
    navigator.clipboard.writeText(json).then(() => {
        Swal.fire({
            toast: true,
            position: 'top-end',
            icon: 'success',
            title: '복사되었습니다',
            showConfirmButton: false,
            timer: 1500
        });
    });
}
