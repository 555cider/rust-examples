// Modal Utility Functions

class Modal {
    constructor(modalId) {
        this.modalId = modalId;
        this.modal = document.getElementById(modalId);
        this.form = document.getElementById(`${modalId}Form`);
        this.titleElement = document.getElementById(`${modalId}Title`);
    }

    open(title = null) {
        if (title && this.titleElement) {
            this.titleElement.textContent = title;
        }
        this.modal.classList.remove('hidden');
        this.modal.classList.add('active');
        document.body.style.overflow = 'hidden';
    }

    close() {
        this.modal.classList.remove('active');
        this.modal.classList.add('hidden');
        document.body.style.overflow = '';
        if (this.form) {
            this.form.reset();
        }
        this.clearErrors();
    }

    clearErrors() {
        const errorElements = this.modal.querySelectorAll('[id$="-error"]');
        errorElements.forEach(el => el.textContent = '');
    }

    showFieldError(fieldName, message) {
        const errorElement = document.getElementById(`${fieldName}-error`);
        if (errorElement) {
            errorElement.textContent = Array.isArray(message) ? message[0] : message;
        }
    }

    showFieldErrors(errors) {
        this.clearErrors();
        for (const [field, message] of Object.entries(errors)) {
            this.showFieldError(field, message);
        }
    }
}

// Close modal on Escape key
document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
        const activeModal = document.querySelector('.modal.active');
        if (activeModal) {
            activeModal.classList.remove('active');
            document.body.style.overflow = '';
        }
    }
});
