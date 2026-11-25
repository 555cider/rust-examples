// Table Utility Functions

class DataTable {
    constructor(tableBodyId, options = {}) {
        this.tableBody = document.getElementById(tableBodyId);
        this.currentPage = 1;
        this.itemsPerPage = options.itemsPerPage || 10;
        this.paginationInfoId = options.paginationInfoId || 'pagination-info';
        this.paginationButtonsId = options.paginationButtonsId || 'pagination-buttons';
    }

    showLoading() {
        const colspan = this.tableBody.closest('table').querySelectorAll('thead th').length;
        this.tableBody.innerHTML = `
            <tr>
                <td colspan="${colspan}" class="loading-state">
                    <i class="fas fa-spinner fa-spin mr-2"></i>
                    데이터를 불러오는 중...
                </td>
            </tr>`;
    }

    showEmpty(message = '표시할 데이터가 없습니다') {
        const colspan = this.tableBody.closest('table').querySelectorAll('thead th').length;
        this.tableBody.innerHTML = `
            <tr>
                <td colspan="${colspan}" class="empty-state">
                    <div class="empty-state-icon">
                        <i class="fas fa-inbox"></i>
                    </div>
                    <p>${message}</p>
                </td>
            </tr>`;
    }

    showError(message = '데이터를 불러오는데 실패했습니다') {
        const colspan = this.tableBody.closest('table').querySelectorAll('thead th').length;
        this.tableBody.innerHTML = `
            <tr>
                <td colspan="${colspan}" class="error-state">
                    <i class="fas fa-exclamation-triangle mr-2"></i>
                    ${message}
                </td>
            </tr>`;
    }

    render(items, renderRowCallback) {
        if (!items || items.length === 0) {
            this.showEmpty();
            return;
        }

        this.tableBody.innerHTML = items.map(renderRowCallback).join('');
    }

    updatePagination(total) {
        const totalPages = Math.ceil(total / this.itemsPerPage);
        const infoElement = document.getElementById(this.paginationInfoId);
        const buttonsElement = document.getElementById(this.paginationButtonsId);

        if (infoElement) {
            const start = (this.currentPage - 1) * this.itemsPerPage + 1;
            const end = Math.min(this.currentPage * this.itemsPerPage, total);
            infoElement.textContent = `총 ${total}개 중 ${start} - ${end}개 표시`;
        }

        if (buttonsElement) {
            let html = '';

            if (this.currentPage > 1) {
                html += `<button onclick="changePage(${this.currentPage - 1})" class="btn-secondary btn-sm">
                    <i class="fas fa-chevron-left"></i>
                </button>`;
            }

            const maxVisiblePages = 5;
            let startPage = Math.max(1, this.currentPage - Math.floor(maxVisiblePages / 2));
            let endPage = Math.min(totalPages, startPage + maxVisiblePages - 1);

            if (endPage - startPage < maxVisiblePages - 1) {
                startPage = Math.max(1, endPage - maxVisiblePages + 1);
            }

            for (let i = startPage; i <= endPage; i++) {
                if (i === this.currentPage) {
                    html += `<button class="btn-primary btn-sm">${i}</button>`;
                } else {
                    html += `<button onclick="changePage(${i})" class="btn-secondary btn-sm">${i}</button>`;
                }
            }

            if (this.currentPage < totalPages) {
                html += `<button onclick="changePage(${this.currentPage + 1})" class="btn-secondary btn-sm">
                    <i class="fas fa-chevron-right"></i>
                </button>`;
            }

            buttonsElement.innerHTML = html;
        }
    }

    setPage(page) {
        this.currentPage = page;
    }

    getPage() {
        return this.currentPage;
    }
}
