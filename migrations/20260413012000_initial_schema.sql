create extension if not exists citext;
create extension if not exists pgcrypto;

create table if not exists users (
    id uuid primary key default gen_random_uuid(),
    username citext not null unique,
    email citext not null unique,
    display_name text,
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists password_credentials (
    user_id uuid primary key references users(id) on delete cascade,
    password_hash text not null,
    password_updated_at timestamptz not null default now(),
    created_at timestamptz not null default now()
);

create table if not exists sessions (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references users(id) on delete cascade,
    session_token_hash text not null unique,
    expires_at timestamptz not null,
    last_seen_at timestamptz not null default now(),
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index if not exists sessions_user_id_idx on sessions (user_id);
create index if not exists sessions_expires_at_idx on sessions (expires_at);

create table if not exists oauth_clients (
    id uuid primary key default gen_random_uuid(),
    client_id text not null unique,
    client_secret_hash text,
    client_name text not null,
    redirect_uris jsonb not null default '[]'::jsonb,
    scopes text[] not null default '{}'::text[],
    grant_types text[] not null default '{}'::text[],
    auth_method text not null default 'client_secret_basic',
    is_confidential boolean not null default true,
    is_active boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists oauth_authorization_codes (
    id uuid primary key default gen_random_uuid(),
    code_hash text not null unique,
    client_id uuid not null references oauth_clients(id) on delete cascade,
    user_id uuid not null references users(id) on delete cascade,
    redirect_uri text not null,
    scope text not null default '',
    nonce text,
    pkce_code_challenge text,
    pkce_code_challenge_method text,
    expires_at timestamptz not null,
    consumed_at timestamptz,
    created_at timestamptz not null default now(),
    check (
        (pkce_code_challenge is null and pkce_code_challenge_method is null)
        or (pkce_code_challenge is not null and pkce_code_challenge_method in ('S256', 'plain'))
    )
);

create index if not exists oauth_authorization_codes_client_id_idx
    on oauth_authorization_codes (client_id);
create index if not exists oauth_authorization_codes_user_id_idx
    on oauth_authorization_codes (user_id);

create table if not exists oauth_refresh_tokens (
    id uuid primary key default gen_random_uuid(),
    refresh_token_hash text not null unique,
    client_id uuid not null references oauth_clients(id) on delete cascade,
    user_id uuid references users(id) on delete set null,
    parent_token_id uuid references oauth_refresh_tokens(id) on delete set null,
    scope text not null default '',
    issued_at timestamptz not null default now(),
    expires_at timestamptz not null,
    rotated_at timestamptz,
    revoked_at timestamptz,
    reused_at timestamptz
);

create index if not exists oauth_refresh_tokens_client_id_idx
    on oauth_refresh_tokens (client_id);
create index if not exists oauth_refresh_tokens_user_id_idx
    on oauth_refresh_tokens (user_id);

create table if not exists oauth_consents (
    id uuid primary key default gen_random_uuid(),
    user_id uuid not null references users(id) on delete cascade,
    client_id uuid not null references oauth_clients(id) on delete cascade,
    scope text not null default '',
    granted_at timestamptz not null default now(),
    expires_at timestamptz,
    unique (user_id, client_id, scope)
);

create table if not exists roles (
    id uuid primary key default gen_random_uuid(),
    role_name text not null unique,
    description text,
    is_system boolean not null default false,
    created_at timestamptz not null default now()
);

create table if not exists permissions (
    id uuid primary key default gen_random_uuid(),
    permission_name text not null unique,
    description text,
    created_at timestamptz not null default now()
);

create table if not exists role_permissions (
    role_id uuid not null references roles(id) on delete cascade,
    permission_id uuid not null references permissions(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (role_id, permission_id)
);

create table if not exists subject_role_bindings (
    id uuid primary key default gen_random_uuid(),
    subject_type text not null,
    subject_id uuid not null,
    role_id uuid not null references roles(id) on delete cascade,
    created_at timestamptz not null default now(),
    unique (subject_type, subject_id, role_id)
);

create index if not exists subject_role_bindings_subject_lookup_idx
    on subject_role_bindings (subject_type, subject_id);

create table if not exists policy_rules (
    id uuid primary key default gen_random_uuid(),
    rule_name text not null unique,
    effect text not null check (effect in ('allow', 'deny')),
    resource_pattern text not null,
    action_pattern text not null,
    condition_expression jsonb not null default '{}'::jsonb,
    priority integer not null default 100,
    is_active boolean not null default true,
    description text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists policy_rules_priority_idx
    on policy_rules (is_active, priority desc);

create table if not exists audit_logs (
    id uuid primary key default gen_random_uuid(),
    actor_type text not null,
    actor_id uuid,
    event_type text not null,
    target_type text,
    target_id uuid,
    ip_address inet,
    user_agent text,
    metadata jsonb not null default '{}'::jsonb,
    occurred_at timestamptz not null default now()
);

create index if not exists audit_logs_occurred_at_idx on audit_logs (occurred_at desc);
