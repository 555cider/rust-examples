/**
 * Dashboard Management Script
 * Handles data fetching, chart updates, and real-time interactions for the dashboard.
 */

let activityChart;
let userDistributionChart;

// Function to show loading state
window.setLoading = function (isLoading) {
    const loadingIndicator = document.getElementById('loading-indicator');
    const refreshIcon = document.getElementById('refresh-icon');
    const refreshText = document.getElementById('refresh-text');
    const refreshBtn = document.getElementById('refresh-btn');

    if (loadingIndicator) loadingIndicator.classList.toggle('hidden', !isLoading);
    if (refreshIcon) refreshIcon.classList.toggle('animate-spin', isLoading);
    if (refreshText) refreshText.textContent = isLoading ? '불러오는 중...' : '새로고침';
    if (refreshBtn) refreshBtn.disabled = isLoading;
}

// Function to show error message
window.showError = function (message) {
    const errorMessage = document.getElementById('error-message');
    const errorText = document.getElementById('error-text');

    if (errorMessage && errorText) {
        errorText.textContent = message || '데이터를 불러오는 중 오류가 발생했습니다.';
        errorMessage.classList.remove('hidden');

        // Hide error after 5 seconds
        setTimeout(() => {
            errorMessage.classList.add('hidden');
        }, 5000);
    }
}

// Function to update dashboard data
window.updateDashboardData = async function (range = 'day') {
    setLoading(true);
    const errorMessage = document.getElementById('error-message');
    if (errorMessage) errorMessage.classList.add('hidden');

    try {
        const url = `/api/dashboard?range=${range}`;

        // Use the global apiClient
        const data = await window.apiClient.get(url, false); // Don't show loading overlay since we have custom loading

        const dashboardData = data.dashboard_data || data;

        // Update stats
        const updateElement = (selector, value) => {
            const element = document.querySelector(selector);
            if (element) {
                // Animate number counting if it's a number
                if (typeof value === 'number') {
                    element.textContent = value.toLocaleString();
                } else {
                    element.textContent = value !== undefined ? value : 0;
                }
            }
        };

        updateElement('[data-stat="total_users"]', dashboardData.total_users);
        updateElement('[data-stat="active_users"]', dashboardData.active_users);

        // Calculate and update active percentage
        const activePercent = dashboardData.active_users && dashboardData.total_users
            ? (dashboardData.active_users / dashboardData.total_users) * 100
            : 0;
        updateElement('[data-stat="active_percent"]', activePercent.toFixed(1));

        // Update charts
        if (typeof updateCharts === 'function') {
            updateCharts(dashboardData);
        }

        // Update recent activities
        if (typeof updateRecentActivities === 'function' && dashboardData.recent_history) {
            updateRecentActivities(dashboardData.recent_history);
        }

    } catch (error) {
        console.error('Error fetching dashboard data:', error);
        showError(error.message || '데이터를 불러오는 중 오류가 발생했습니다.');
    } finally {
        setLoading(false);
    }
}

// Function to update recent activities list
window.updateRecentActivities = function (activities) {
    const activitiesList = document.getElementById('recentActivities');
    if (!activitiesList) return;

    if (!activities || activities.length === 0) {
        activitiesList.innerHTML = `
            <div class="p-8 text-center text-gray-500 flex flex-col items-center">
                <div class="h-16 w-16 bg-gray-50 rounded-full flex items-center justify-center mb-4">
                    <i class="fas fa-inbox text-2xl text-gray-300"></i>
                </div>
                <p>활동 내역이 없습니다.</p>
            </div>`;
        return;
    }

    activitiesList.innerHTML = activities.map(activity => `
        <div class="p-4 hover:bg-gray-50/80 transition-colors border-b border-gray-50 last:border-0">
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <div class="flex-shrink-0">
                        <div class="h-10 w-10 rounded-full bg-gradient-to-br from-gray-100 to-gray-200 flex items-center justify-center shadow-inner">
                            <i class="fas fa-user text-gray-500 text-sm"></i>
                        </div>
                    </div>
                    <div>
                        <p class="text-sm font-semibold text-gray-900">${activity.username || 'System'}</p>
                        <p class="text-xs text-gray-500 flex items-center gap-2">
                            <span class="px-1.5 py-0.5 rounded bg-gray-100 text-gray-600 font-medium">${activity.action}</span>
                            <span>•</span>
                            <span>${new Date(activity.created_at).toLocaleString()}</span>
                        </p>
                    </div>
                </div>
                <div class="text-right">
                    <span class="text-xs font-mono text-gray-400 bg-gray-50 px-2 py-1 rounded border border-gray-100">
                        ${activity.ip_address || 'N/A'}
                    </span>
                </div>
            </div>
        </div>
    `).join('');
}

// Function to update charts
window.updateCharts = function (data) {
    // Destroy existing charts if they exist
    if (activityChart) {
        activityChart.destroy();
    }
    if (userDistributionChart) {
        userDistributionChart.destroy();
    }

    // Activity Chart
    const activityCanvas = document.getElementById('activityChart');
    if (activityCanvas) {
        const activityCtx = activityCanvas.getContext('2d');
        const activityLabels = data.daily_active_users?.map(item => item.date) || [];
        const activityData = data.daily_active_users?.map(item => item.count) || [];

        // Create gradient with purple theme
        const gradient = activityCtx.createLinearGradient(0, 0, 0, 400);
        gradient.addColorStop(0, 'rgba(139, 92, 246, 0.5)'); // Primary-500
        gradient.addColorStop(1, 'rgba(139, 92, 246, 0.0)');

        activityChart = new Chart(activityCtx, {
            type: 'line',
            data: {
                labels: activityLabels,
                datasets: [{
                    label: '활성 사용자',
                    data: activityData,
                    borderColor: '#8b5cf6', // Primary-500 purple
                    backgroundColor: gradient,
                    borderWidth: 3,
                    tension: 0.4,
                    fill: true,
                    pointBackgroundColor: '#ffffff',
                    pointBorderColor: '#8b5cf6',
                    pointBorderWidth: 2,
                    pointRadius: 4,
                    pointHoverRadius: 6
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        mode: 'index',
                        intersect: false,
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        titleColor: '#18181b',
                        bodyColor: '#52525b',
                        borderColor: '#e4e4e7',
                        borderWidth: 1,
                        padding: 10,
                        displayColors: false,
                        callbacks: {
                            label: function (context) {
                                return context.parsed.y + ' 명';
                            }
                        }
                    }
                },
                scales: {
                    y: {
                        beginAtZero: true,
                        grid: {
                            color: '#f4f4f5',
                            drawBorder: false
                        },
                        ticks: {
                            precision: 0,
                            color: '#a1a1aa',
                            font: {
                                family: "'Inter', sans-serif"
                            }
                        }
                    },
                    x: {
                        grid: {
                            display: false
                        },
                        ticks: {
                            color: '#a1a1aa',
                            font: {
                                family: "'Inter', sans-serif"
                            }
                        }
                    }
                },
                interaction: {
                    mode: 'nearest',
                    axis: 'x',
                    intersect: false
                }
            }
        });
    }

    // User Distribution Chart
    const userDistCanvas = document.getElementById('userDistributionChart');
    if (userDistCanvas) {
        const userDistCtx = userDistCanvas.getContext('2d');
        userDistributionChart = new Chart(userDistCtx, {
            type: 'doughnut',
            data: {
                labels: ['활성 사용자', '비활성 사용자'],
                datasets: [{
                    data: [data.active_users || 0, data.inactive_users || 0],
                    backgroundColor: [
                        '#8b5cf6', // Primary-500
                        '#e4e4e7'  // Gray-200
                    ],
                    borderColor: [
                        '#ffffff',
                        '#ffffff'
                    ],
                    borderWidth: 2,
                    hoverOffset: 4
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                cutout: '75%',
                plugins: {
                    legend: {
                        position: 'bottom',
                        labels: {
                            usePointStyle: true,
                            padding: 20,
                            font: {
                                family: "'Inter', sans-serif",
                                size: 12
                            },
                            color: '#52525b'
                        }
                    },
                    tooltip: {
                        backgroundColor: 'rgba(255, 255, 255, 0.9)',
                        bodyColor: '#18181b',
                        borderColor: '#e4e4e7',
                        borderWidth: 1,
                        padding: 10,
                        callbacks: {
                            label: function (context) {
                                const label = context.label || '';
                                const value = context.parsed || 0;
                                const total = context.dataset.data.reduce((a, b) => a + b, 0);
                                const percentage = total > 0 ? Math.round((value / total) * 100) + '%' : '0%';
                                return `${label}: ${value} (${percentage})`;
                            }
                        }
                    }
                }
            }
        });
    }
}

// Handle time range button clicks
window.setupRangeButtons = function () {
    const buttons = document.querySelectorAll('.range-btn');
    buttons.forEach(button => {
        button.addEventListener('click', function () {
            // Update active state
            buttons.forEach(btn => {
                btn.classList.remove('bg-primary-100', 'text-primary-700', 'border-primary-500');
                btn.classList.add('bg-white', 'text-gray-700');
            });
            this.classList.add('bg-primary-100', 'text-primary-700', 'border-primary-500');
            this.classList.remove('bg-white', 'text-gray-700');

            // Update dashboard with selected range
            const range = this.getAttribute('data-range');
            updateDashboardData(range);
        });
    });

    // Set default active button
    const defaultBtn = document.querySelector('.range-btn[data-range="day"]');
    if (defaultBtn) {
        defaultBtn.classList.add('bg-primary-100', 'text-primary-700', 'border-primary-500');
        defaultBtn.classList.remove('bg-white', 'text-gray-700');
    }
}

// Initialize dashboard
window.initDashboard = function (initialData) {
    setupRangeButtons();

    if (initialData && initialData.daily_active_users) {
        // Use server-side data for initial render
        try {
            // Update stats from server data
            const updateElement = (selector, value) => {
                const element = document.querySelector(selector);
                if (element) {
                    if (typeof value === 'number') {
                        element.textContent = value.toLocaleString();
                    } else {
                        element.textContent = value !== undefined ? value : 0;
                    }
                }
            };

            updateElement('[data-stat="total_users"]', initialData.total_users);
            updateElement('[data-stat="active_users"]', initialData.active_users);

            const activePercent = initialData.active_users && initialData.total_users
                ? (initialData.active_users / initialData.total_users) * 100
                : 0;
            updateElement('[data-stat="active_percent"]', activePercent.toFixed(1));

            updateCharts(initialData);
            if (initialData.recent_history) {
                updateRecentActivities(initialData.recent_history);
            }
        } catch (e) {
            console.error('Error initializing with server data:', e);
            // Fallback to fetching data
            updateDashboardData('day');
        }
    } else {
        // No initial data, fetch from API
        updateDashboardData('day');
    }

    // Refresh data every 5 minutes
    setInterval(() => updateDashboardData('day'), 5 * 60 * 1000);
}
