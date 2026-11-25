-- Consolidated Schema Migration
-- Created based on current schema state

PRAGMA foreign_keys = ON;

-- =============================================
-- 1. Roles (formerly user_type)
-- =============================================
CREATE TABLE IF NOT EXISTS "roles" (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    is_active   BOOLEAN DEFAULT TRUE NOT NULL,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TRIGGER IF NOT EXISTS role_updated_at
    AFTER UPDATE ON "roles"
    FOR EACH ROW
BEGIN
    UPDATE "roles" SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 2. Permissions
-- =============================================
CREATE TABLE IF NOT EXISTS "permission" (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    code        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT,
    category    TEXT DEFAULT 'system',
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE TRIGGER IF NOT EXISTS permission_updated_at
    AFTER UPDATE ON permission
    FOR EACH ROW
BEGIN
    UPDATE permission SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 3. Role - Permission Mapping
-- =============================================
CREATE TABLE IF NOT EXISTS "role_permissions" (
    role_id  INTEGER NOT NULL REFERENCES "roles" (id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permission (id) ON DELETE CASCADE,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    PRIMARY KEY (role_id, permission_id)
);

CREATE INDEX IF NOT EXISTS idx_role_permissions_role ON "role_permissions"(role_id);
CREATE INDEX IF NOT EXISTS idx_role_permissions_permission ON "role_permissions"(permission_id);

-- =============================================
-- 4. Admin Users
-- =============================================
CREATE TABLE IF NOT EXISTS admin_user (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    email         TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    role_id       INTEGER NOT NULL REFERENCES "roles" (id) ON DELETE RESTRICT,
    is_active     BOOLEAN DEFAULT TRUE NOT NULL,
    last_login_at DATETIME,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at    DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    attributes    TEXT DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_admin_user_username ON admin_user(username);
CREATE INDEX IF NOT EXISTS idx_admin_user_email ON admin_user(email);
CREATE INDEX IF NOT EXISTS idx_admin_user_role_id ON admin_user(role_id);

CREATE TRIGGER IF NOT EXISTS admin_user_updated_at
    AFTER UPDATE ON admin_user
    FOR EACH ROW
BEGIN
    UPDATE admin_user SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 5. Refresh Tokens
-- =============================================
CREATE TABLE IF NOT EXISTS user_refresh_token (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER NOT NULL REFERENCES admin_user (id) ON DELETE CASCADE,
    refresh_token TEXT NOT NULL,
    expires_at    DATETIME NOT NULL,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at    DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    UNIQUE(user_id)
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_user_id ON user_refresh_token (user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_token ON user_refresh_token (refresh_token);

-- =============================================
-- 6. History/Audit Log
-- =============================================
CREATE TABLE IF NOT EXISTS history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER REFERENCES admin_user (id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    entity_id   INTEGER,
    details     TEXT,
    ip_address  TEXT,
    user_agent  TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_history_user_id ON history (user_id);
CREATE INDEX IF NOT EXISTS idx_history_created_at ON history (created_at);
CREATE INDEX IF NOT EXISTS idx_history_entity_id ON history (entity_id);

-- =============================================
-- 7. Access Evaluation Log
-- =============================================
CREATE TABLE IF NOT EXISTS access_evaluation_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    user_id INTEGER REFERENCES admin_user(id) ON DELETE SET NULL,
    resource_id TEXT,
    resource_type TEXT,
    action TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('PERMIT', 'DENY', 'NOT_APPLICABLE')),
    rbac_result TEXT CHECK (rbac_result IN ('PERMIT', 'DENY', 'NOT_APPLICABLE')),
    abac_result TEXT CHECK (abac_result IN ('PERMIT', 'DENY', 'NOT_APPLICABLE')),
    applied_policies TEXT, -- JSON array of policy IDs
    evaluation_time_ms INTEGER,
    context_json TEXT, -- JSON context data
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_user_id ON access_evaluation_log (user_id);
CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_created_at ON access_evaluation_log (created_at);
CREATE INDEX IF NOT EXISTS idx_access_evaluation_log_request_id ON access_evaluation_log (request_id);

-- =============================================
-- 8. ABAC Policies
-- =============================================
CREATE TABLE IF NOT EXISTS abac_policy (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    target_json TEXT NOT NULL,
    rules_json TEXT NOT NULL,
    effect TEXT NOT NULL CHECK (effect IN ('PERMIT', 'DENY')),
    priority INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_abac_policy_is_active ON abac_policy (is_active);
CREATE INDEX IF NOT EXISTS idx_abac_policy_priority ON abac_policy (priority DESC);

CREATE TRIGGER IF NOT EXISTS abac_policy_updated_at
    AFTER UPDATE ON abac_policy
    FOR EACH ROW
BEGIN
    UPDATE abac_policy SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 9. User Attributes (ABAC)
-- =============================================
CREATE TABLE IF NOT EXISTS user_attribute (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES admin_user(id) ON DELETE CASCADE,
    attribute_name TEXT NOT NULL,
    attribute_value TEXT NOT NULL,
    attribute_type TEXT NOT NULL CHECK (attribute_type IN ('STRING', 'NUMBER', 'BOOLEAN', 'DATE', 'JSON')),
    valid_from DATETIME,
    valid_until DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    UNIQUE(user_id, attribute_name)
);

CREATE INDEX IF NOT EXISTS idx_user_attribute_user_id ON user_attribute (user_id);
CREATE INDEX IF NOT EXISTS idx_user_attribute_name ON user_attribute (attribute_name);

CREATE TRIGGER IF NOT EXISTS user_attribute_updated_at
    AFTER UPDATE ON user_attribute
    FOR EACH ROW
BEGIN
    UPDATE user_attribute SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 10. Resource Attributes (ABAC)
-- =============================================
CREATE TABLE IF NOT EXISTS resource_attribute (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    attribute_name TEXT NOT NULL,
    attribute_value TEXT NOT NULL,
    attribute_type TEXT NOT NULL CHECK (attribute_type IN ('STRING', 'NUMBER', 'BOOLEAN', 'DATE', 'JSON')),
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP NOT NULL,
    UNIQUE(resource_id, resource_type, attribute_name)
);

CREATE INDEX IF NOT EXISTS idx_resource_attribute_resource ON resource_attribute (resource_id, resource_type);

CREATE TRIGGER IF NOT EXISTS resource_attribute_updated_at
    AFTER UPDATE ON resource_attribute
    FOR EACH ROW
BEGIN
    UPDATE resource_attribute SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- =============================================
-- 11. Schema Migrations (Custom)
-- =============================================
CREATE TABLE IF NOT EXISTS schema_migrations (
    version BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- =============================================
-- Initial Data
-- =============================================

-- Insert default roles
INSERT INTO roles (code, name, description, is_active)
VALUES 
    ('super_admin', 'Super Administrator', 'Has full access to all features', 1),
    ('admin', 'Administrator', 'Has most administrative privileges', 1),
    ('manager', 'Manager', 'Has limited administrative access', 1),
    ('user', 'Standard User', 'Basic user with minimal permissions', 1)
ON CONFLICT(code) DO NOTHING;

-- Insert default permissions
INSERT INTO permission (code, name, description, category)
VALUES 
    -- User Management
    ('user:read', 'View Users', 'View user accounts', 'user'),
    ('user:create', 'Create Users', 'Create new user accounts', 'user'),
    ('user:update', 'Edit Users', 'Edit existing user accounts', 'user'),
    ('user:delete', 'Delete Users', 'Delete user accounts', 'user'),
    
    -- Role Management
    ('role:read', 'View Roles', 'View user roles and permissions', 'role'),
    ('role:create', 'Create Roles', 'Create new user roles', 'role'),
    ('role:update', 'Edit Roles', 'Edit existing user roles', 'role'),
    ('role:delete', 'Delete Roles', 'Delete user roles', 'role'),
    ('role:assign', 'Assign Roles', 'Assign roles to users', 'role'),
    
    -- System Settings
    ('settings:read', 'View Settings', 'View system settings', 'system'),
    ('settings:update', 'Update Settings', 'Update system settings', 'system'),
    
    -- Audit Logs
    ('audit:read', 'View Audit Logs', 'View system audit logs', 'audit')
ON CONFLICT(code) DO NOTHING;

-- Assign all permissions to super admin
INSERT INTO role_permissions (role_id, permission_id)
SELECT 
    r.id, 
    p.id 
FROM 
    roles r, 
    permission p 
WHERE 
    r.code = 'super_admin' 
    AND p.code IN (
        'user:read', 'user:create', 'user:update', 'user:delete',
        'role:read', 'role:create', 'role:update', 'role:delete', 'role:assign',
        'settings:read', 'settings:update',
        'audit:read'
    )
ON CONFLICT(role_id, permission_id) DO NOTHING;

-- Create default admin user (password: admin123)
INSERT INTO admin_user (username, password_hash, role_id, is_active)
SELECT 
    'admin', 
    '$2a$12$8K3mitisuL4ddezwjDsx7O/9lLVOrCXPRJJxDWYIjuwyJFpRwJGKq',
    (SELECT id FROM roles WHERE code = 'super_admin' LIMIT 1),
    1
WHERE NOT EXISTS (SELECT 1 FROM admin_user WHERE username = 'admin');

-- Initialize first audit log entry
INSERT INTO history (user_id, action, entity_id, details, ip_address, user_agent)
VALUES (
    (SELECT id FROM admin_user WHERE username = 'admin' LIMIT 1),
    'SYSTEM_INIT',
    NULL,
    'System initialized with default admin user and permissions',
    '127.0.0.1',
    'System/1.0'
);

-- Mark this migration as applied
INSERT OR IGNORE INTO schema_migrations (version, name) 
VALUES (20251127000000, 'consolidated_schema');
