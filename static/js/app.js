/**
 * PrefixDDNS Dashboard Application
 * Handles SSE connection, task management, and UI interactions.
 */

const App = {
    state: {
        config: {
            tasks: [],
            log_limit: 100,
            run_on_startup: true
        },
        sse: null,
        reconnectTimer: null,
        currentTaskIndex: -1, // -1 for new task
        logCount: 0,
        isDirty: false
    },

    // Defined Templates (Source of Truth)
    templates: {
        'webhook': {
             name: 'Generic Webhook',
             webhook_method: 'POST',
             webhook_url: 'https://example.com/webhook',
             webhook_headers: { 'Content-Type': 'application/json' },
             webhook_body: JSON.stringify({ ip: '{{combined_ip}}' }, null, 2),
             suffix: ''
        },        
        'cloudflare': {
            name: 'Cloudflare DNS',
            webhook_method: 'PUT',
            webhook_url: 'https://api.cloudflare.com/client/v4/zones/YOUR_ZONE_ID/dns_records/YOUR_RECORD_ID',
            webhook_headers: { 'Authorization': 'Bearer YOUR_TOKEN', 'Content-Type': 'application/json' },
            webhook_body: JSON.stringify({ type: 'AAAA', name: 'example.com', content: '{{combined_ip}}', ttl: 120, proxied: false }, null, 2),
            suffix: '::1'
        },
        'dynv6': {
            name: 'Dynv6 (Zone)',
            webhook_method: 'GET',
            webhook_url: 'https://dynv6.com/api/update?hostname=YOUR_HOSTNAME&token=YOUR_TOKEN&ipv6={{combined_ip}}',
            webhook_headers: {},
            webhook_body: null,
            suffix: ''
        },
        'dynv6_subdomain': {
            name: 'Dynv6 (Subdomain)',
            webhook_method: 'PATCH',
            webhook_url: 'https://dynv6.com/api/v2/zones/YOUR_ZONE_ID/records/YOUR_RECORD_ID',
            webhook_headers: { 'Authorization': 'Bearer YOUR_TOKEN', 'Content-Type': 'application/json' },
            webhook_body: JSON.stringify({ data: '{{combined_ip}}' }, null, 2),
            suffix: ''
        },
        'dynu': {
            name: 'Dynu (Zone)',
            webhook_method: 'GET',
            webhook_url: 'https://api.dynu.com/nic/update?hostname=YOUR_HOSTNAME&myipv6={{combined_ip}}&username=YOUR_USERNAME&password=YOUR_PASSWORD',
            webhook_headers: {},
            webhook_body: null,
            suffix: ''
        },
        'dynu_subdomain': {
            name: 'Dynu (Subdomain/Alias)',
            webhook_method: 'GET',
            webhook_url: 'https://api.dynu.com/nic/update?hostname=YOUR_ROOT_DOMAIN&alias=YOUR_SUBDOMAIN&myipv6={{combined_ip}}&username=YOUR_USERNAME&password=YOUR_PASSWORD',
            webhook_headers: {},
            webhook_body: null,
            suffix: ''
        },
        'afraid': {
            name: 'Afraid.org (FreeDNS)',
            webhook_method: 'GET',
            webhook_url: 'https://freedns.afraid.org/dynamic/update.php?YOUR_TOKEN&address={{combined_ip}}',
            webhook_headers: {},
            webhook_body: null,
            suffix: ''
        },
        'duckdns': {
            name: 'DuckDNS',
            webhook_method: 'GET',
            webhook_url: 'https://www.duckdns.org/update?domains=YOUR_DOMAIN&token=YOUR_TOKEN&ipv6={{combined_ip}}',
            webhook_headers: {},
            webhook_body: null,
            suffix: ''
        },
        'desec': {
            name: 'deSEC.io',
            webhook_method: 'GET',
            webhook_url: 'https://update.dedyn.io/?hostname=YOUR_FULL_DOMAIN&myipv6={{combined_ip}}',
            webhook_headers: { 'Authorization': 'Token YOUR_TOKEN' },
            webhook_body: null,
            suffix: ''
        },
        'ydns': {
            name: 'YDNS',
            webhook_method: 'GET',
            webhook_url: 'https://ydns.io/api/v1/update/?host=YOUR_HOST&ip={{combined_ip}}',
            webhook_headers: { 'Authorization': 'Basic YOUR_BASE64_AUTH' },
            webhook_body: null,
            suffix: ''
        }
    },

    elements: {
        taskList: document.getElementById('tasks-list'),
        taskTemplate: document.getElementById('task-item-template'),
        noTasksMsg: document.getElementById('no-tasks-msg'),
        templateSelector: document.getElementById('template-selector'),
        
        // Modal
        modal: document.getElementById('task-modal'),
        modalTitle: document.querySelector('.modal-title'),
        modalInputs: {
            id: document.getElementById('modal-task-id'),
            name: document.getElementById('modal-task-name'),
            suffix: document.getElementById('modal-task-suffix'),
            method: document.getElementById('modal-task-method'),
            url: document.getElementById('modal-task-url'),
            headers: document.getElementById('modal-task-headers'),
            body: document.getElementById('modal-task-body')
        },
        
        // Settings
        settingLogLimit: document.getElementById('setting-log-limit'),
        settingRunOnStartup: document.getElementById('setting-run-on-startup'),
        
        // Logs
        logsOutput: document.getElementById('logs-output'),
        logCount: document.getElementById('log-count'),
        connectionDot: document.getElementById('connection-dot'),
        connectionText: document.getElementById('connection-text')
    },

    markDirty() {
        if (this.state.isDirty) return;
        this.state.isDirty = true;
        const btn = document.getElementById('global-save-btn');
        btn.classList.add('btn-warning');
        btn.classList.remove('btn-primary');
        // Add indicator text without removing icon
        if (!btn.querySelector('.unsaved-indicator')) {
            const span = document.createElement('span');
            span.className = 'unsaved-indicator';
            span.textContent = '*';
            span.style.marginLeft = '4px';
            span.style.fontWeight = 'bold';
            btn.appendChild(span);
        }
        
        // Add beforeunload listener
        window.onbeforeunload = (e) => {
            e.preventDefault();
            e.returnValue = 'You have unsaved changes. Are you sure you want to leave?';
        };
    },

    markClean() {
        this.state.isDirty = false;
        const btn = document.getElementById('global-save-btn');
        btn.classList.remove('btn-warning');
        btn.classList.add('btn-primary');
        const indicator = btn.querySelector('.unsaved-indicator');
        if (indicator) indicator.remove();
        
        // Remove beforeunload listener
        window.onbeforeunload = null;
    },

    init() {
        this.initTemplates();
        this.connectSSE();
        this.fetchConfig();
        this.setupEventListeners();
    },

    initTemplates() {
        const selector = this.elements.templateSelector;
        if (!selector) return;

        // Clear existing options
        selector.innerHTML = '';

        // Add Empty Template (Default)
        const emptyOption = document.createElement('option');
        emptyOption.value = 'empty';
        emptyOption.textContent = 'Empty Template';
        selector.appendChild(emptyOption);

        // Add Defined Templates
        Object.keys(this.templates).forEach(key => {
            const template = this.templates[key];
            const option = document.createElement('option');
            option.value = key;
            option.textContent = template.name;
            selector.appendChild(option);
        });
    },

    setupEventListeners() {
        // Global Buttons
        document.getElementById('global-save-btn').addEventListener('click', () => this.saveConfig());
        document.getElementById('add-task-btn').addEventListener('click', () => this.openTaskModal(-1));
        document.getElementById('clear-logs-btn').addEventListener('click', () => this.clearLogs());

        // Modal Buttons
        // Unified Done Button (Saves to memory & Closes)
        document.getElementById('modal-save-btn').addEventListener('click', () => this.updateTaskFromModal());
        document.getElementById('modal-delete-btn').addEventListener('click', () => this.deleteTask());
        document.getElementById('modal-test-btn').addEventListener('click', () => this.testRunTask());
        document.getElementById('modal-copy-btn').addEventListener('click', () => this.copyTask());

        // Click outside modal to close (Cancel)
        window.addEventListener('click', (e) => {
            if (e.target === this.elements.modal) {
                this.closeModal();
            }
        });
    },

    async fetchConfig() {
        try {
            const response = await fetch('/api/config');
            if (!response.ok) throw new Error('Failed to load config');
            const data = await response.json();
            
            this.state.config = { ...this.state.config, ...data };
            
            // Update Settings UI
        this.elements.settingLogLimit.value = this.state.config.log_limit || 100;
        this.elements.settingRunOnStartup.checked = !!this.state.config.run_on_startup;

        // Settings change listeners
        this.elements.settingLogLimit.addEventListener('change', () => this.markDirty());
        this.elements.settingRunOnStartup.addEventListener('change', () => this.markDirty());

        this.renderTasks();
            this.showToast('Configuration loaded', 'success');
        } catch (error) {
            console.error('Error fetching config:', error);
            this.showToast('Error loading configuration', 'error');
        }
    },

    connectSSE() {
        this.updateConnectionStatus('connecting');

        if (this.state.sse) {
            this.state.sse.close();
        }

        this.state.sse = new EventSource('/events');

        this.state.sse.onopen = () => {
            this.updateConnectionStatus('connected');
            if (this.state.reconnectTimer) {
                clearTimeout(this.state.reconnectTimer);
                this.state.reconnectTimer = null;
            }
        };

        this.state.sse.onmessage = (event) => {
            try {
                const logEntry = JSON.parse(event.data);
                this.appendLog(logEntry);
            } catch (e) {
                console.error('SSE message error:', e);
            }
        };

        this.state.sse.onerror = () => {
            this.updateConnectionStatus('disconnected');
            this.state.sse.close();
            if (!this.state.reconnectTimer) {
                this.state.reconnectTimer = setTimeout(() => this.connectSSE(), 3000);
            }
        };
    },

    updateConnectionStatus(status) {
        const dot = this.elements.connectionDot;
        const text = this.elements.connectionText;
        
        dot.className = 'status-dot'; // Reset
        
        switch(status) {
            case 'connected':
                dot.classList.add('connected');
                text.textContent = 'Connected';
                break;
            case 'connecting':
                dot.classList.add('connecting');
                text.textContent = 'Connecting...';
                break;
            case 'disconnected':
                dot.classList.add('disconnected');
                text.textContent = 'Disconnected';
                break;
        }
    },

    renderTasks() {
        const list = this.elements.taskList;
        list.innerHTML = '';
        
        const tasks = this.state.config.tasks || [];

        if (tasks.length === 0) {
            this.elements.noTasksMsg.style.display = 'block';
            return;
        } else {
            this.elements.noTasksMsg.style.display = 'none';
        }

        tasks.forEach((task, index) => {
            const clone = this.elements.taskTemplate.content.cloneNode(true);
            const card = clone.querySelector('.task-card');
            
            // Name
            clone.querySelector('.task-name').textContent = task.name || 'Unnamed Task';
            // Suffix and URL removed from display


            // Status Styling
            if (!task.enabled) {
                card.classList.add('disabled');
            }

            // Click entire card to edit
            card.addEventListener('click', (e) => {
                this.openTaskModal(index);
            });

            // --- Controls on Card ---

            // 1. Enable Toggle
            const enableToggle = clone.querySelector('.task-enable-toggle');
            enableToggle.checked = !!task.enabled;
            enableToggle.addEventListener('change', (e) => {
                this.toggleTaskEnabled(index, e.target.checked);
            });

            // 2. API Trigger Toggle (Now Checkbox)
            const apiToggle = clone.querySelector('.task-api-toggle');
            apiToggle.checked = !!task.allow_api_trigger;
            apiToggle.addEventListener('change', (e) => {
                this.toggleTaskApi(index, e.target.checked);
            });

            list.appendChild(clone);
        });
    },

    toggleTaskEnabled(index, isEnabled) {
        this.state.config.tasks[index].enabled = isEnabled;
        this.renderTasks(); 
        this.markDirty();
    },

    toggleTaskApi(index, isAllowed) {
        this.state.config.tasks[index].allow_api_trigger = isAllowed;
        this.markDirty();
    },

    openTaskModal(index) {
        this.state.currentTaskIndex = index;
        const isNew = index === -1;
        
        this.elements.modalTitle.textContent = isNew ? 'Add New Task' : 'Edit Task Details';
        document.getElementById('modal-delete-btn').style.display = isNew ? 'none' : 'block';
        document.getElementById('modal-copy-btn').style.display = isNew ? 'none' : 'block';
        document.getElementById('modal-save-btn').textContent = isNew ? 'Add Task' : 'Done';

        // Default Data
        const task = isNew ? this.getTemplateData() : this.state.config.tasks[index];

        // Populate Inputs
        const inputs = this.elements.modalInputs;
        inputs.name.value = task.name || '';
        inputs.suffix.value = task.suffix || ''; 
        inputs.method.value = task.webhook_method || 'GET';
        inputs.url.value = task.webhook_url || '';
        inputs.headers.value = this.objToString(task.webhook_headers);
        inputs.body.value = task.webhook_body || '';

        // Show Modal
        this.elements.modal.style.display = 'flex';
    },

    updateTaskFromModal() {
        const inputs = this.elements.modalInputs;
        
        // Validate
        if (!inputs.name.value.trim()) {
            this.showToast('Task name is required', 'error');
            return;
        }

        // Get existing task to preserve ID and toggle states
        const existingTask = this.state.currentTaskIndex !== -1 ? this.state.config.tasks[this.state.currentTaskIndex] : null;

        const taskData = {
            id: existingTask ? existingTask.id : this.generateId(),
            name: inputs.name.value.trim(),
            suffix: inputs.suffix.value.trim(),
            enabled: existingTask ? existingTask.enabled : true, // Preserve or Default
            allow_api_trigger: existingTask ? existingTask.allow_api_trigger : false, // Preserve or Default
            webhook_method: inputs.method.value,
            webhook_url: inputs.url.value.trim(),
            webhook_headers: this.stringToObj(inputs.headers.value),
            webhook_body: inputs.body.value || null
        };

        if (this.state.currentTaskIndex === -1) {
            this.state.config.tasks.push(taskData);
        } else {
            this.state.config.tasks[this.state.currentTaskIndex] = taskData;
        }

        this.renderTasks();
        this.closeModal();
        this.markDirty();
        this.showToast(this.state.currentTaskIndex === -1 ? 'Task added (unsaved)' : 'Task updated (unsaved)');
    },

    copyTask() {
        if (this.state.currentTaskIndex === -1) return;

        const task = this.state.config.tasks[this.state.currentTaskIndex];
        const newTask = JSON.parse(JSON.stringify(task)); // Deep copy
        
        newTask.id = this.generateId();
        newTask.name = `${newTask.name} (Copy)`;
        
        this.state.config.tasks.push(newTask);
        this.renderTasks();
        this.closeModal();
        this.markDirty();
        this.showToast('Task copied (unsaved)');
    },

    deleteTask() {
        if (this.state.currentTaskIndex === -1) return;
        
        if (confirm('Are you sure you want to delete this task?')) {
            this.state.config.tasks.splice(this.state.currentTaskIndex, 1);
            this.renderTasks();
            this.closeModal();
            this.markDirty();
            this.showToast('Task deleted (unsaved)');
        }
    },

    closeModal() {
        this.elements.modal.style.display = 'none';
        this.state.currentTaskIndex = -1;
    },

    async saveConfig() {
        // Update global settings from UI
        this.state.config.log_limit = Math.max(1, parseInt(this.elements.settingLogLimit.value) || 100);
        this.state.config.run_on_startup = this.elements.settingRunOnStartup.checked;

        try {
            const response = await fetch('/api/config', {
                method: 'POST',
                headers: { 
                    'Content-Type': 'application/json',
                    'Accept': 'application/json'
                },
                body: JSON.stringify(this.state.config)
            });

            if (response.ok) {
                this.markClean();
                this.showToast('Configuration saved successfully!', 'success');
            } else {
                const text = await response.text();
                throw new Error('Save failed: ' + text);
            }
        } catch (error) {
            this.showToast('Error saving configuration', 'error');
            console.error(error);
        }
    },

    async testRunTask() {
        // Run test with current modal data
        const inputs = this.elements.modalInputs;
        
        const taskData = {
            id: "test",
            name: inputs.name.value,
            suffix: inputs.suffix.value,
            webhook_method: inputs.method.value,
            webhook_url: inputs.url.value,
            webhook_headers: this.stringToObj(inputs.headers.value),
            webhook_body: inputs.body.value || null,
            enabled: true,
            allow_api_trigger: true
        };

        const payload = {
            task: taskData,
            fake_ip: "2001:db8::1" // Default test IP
        };

        try {
            this.showToast('Sending test request...', 'info');
            const response = await fetch('/api/test-webhook', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload)
            });
            
            const resultText = await response.text();
            
            if (response.ok) {
                this.showToast('Test Result: ' + resultText, 'success');
            } else {
                this.showToast('Test Failed: ' + resultText, 'error');
            }
        } catch (error) {
            this.showToast('Error executing test run', 'error');
            console.error(error);
        }
    },

    // Utilities
    generateId() {
        return Math.random().toString(36).substr(2, 9);
    },

    getTemplateData() {
        const selector = this.elements.templateSelector;
        const type = selector ? selector.value : 'empty';
        
        if (type === 'empty') {
            return {
                name: '',
                webhook_method: 'GET',
                webhook_url: '',
                webhook_headers: {},
                webhook_body: '',
                suffix: ''
            };
        }

        const template = this.templates[type];
        if (template) {
            // Return a copy to avoid mutation
            return JSON.parse(JSON.stringify(template));
        }

        // Fallback
        return {
            name: '',
            webhook_method: 'GET',
            webhook_url: '',
            webhook_headers: {},
            webhook_body: '',
            suffix: ''
        };
    },

    stringToObj(str) {
        try {
            const obj = {};
            str.split('\n').forEach(line => {
                const [key, ...val] = line.split(':');
                if (key && val) obj[key.trim()] = val.join(':').trim();
            });
            return obj;
        } catch (e) { return {}; }
    },

    objToString(obj) {
        if (!obj) return '';
        return Object.entries(obj).map(([k, v]) => `${k}: ${v}`).join('\n');
    },

    appendLog(logData) {
        const div = document.createElement('div');
        div.className = `log-entry log-${logData.level.toLowerCase()}`;
        
        // Use the raw timestamp string from backend for full precision/format
        const timeStr = logData.timestamp;
        
        div.innerHTML = `<span class="log-time">[${timeStr}]</span> <span class="log-source">[${logData.source || 'UNK'}]</span> <span class="log-level ${logData.level.toLowerCase()}">${logData.level}</span> <span class="log-msg">${this.escapeHtml(logData.message)}</span>`;
        
        // Prepend new logs to the top
        this.elements.logsOutput.insertBefore(div, this.elements.logsOutput.firstChild);
        
        this.state.logCount++;
        this.elements.logCount.textContent = `${this.state.logCount} logs`;
    },

    clearLogs() {
        this.elements.logsOutput.innerHTML = '';
        this.state.logCount = 0;
        this.elements.logCount.textContent = '0 logs';
    },

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    },

    showToast(message, type = 'info') {
        const container = document.getElementById('toast-container');
        const toast = document.createElement('div');
        toast.className = `toast toast-${type}`;
        toast.textContent = message;
        
        container.appendChild(toast);
        
        setTimeout(() => toast.classList.add('show'), 10);
        
        setTimeout(() => {
            toast.classList.remove('show');
            setTimeout(() => container.removeChild(toast), 300);
        }, 3000);
    }
};

document.addEventListener('DOMContentLoaded', () => {
    App.init();
});
