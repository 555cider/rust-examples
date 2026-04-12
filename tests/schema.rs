#[sqlx::test(migrator = "auth_server::db::MIGRATOR")]
async fn migrations_create_core_auth_tables(pool: sqlx::PgPool) {
    let table_names: Vec<String> = sqlx::query_scalar(
        r#"
        select table_name
        from information_schema.tables
        where table_schema = 'public'
          and table_name = any($1)
        order by table_name
        "#,
    )
    .bind([
        "audit_logs",
        "oauth_authorization_codes",
        "oauth_clients",
        "oauth_consents",
        "oauth_refresh_tokens",
        "password_credentials",
        "permissions",
        "policy_rules",
        "role_permissions",
        "roles",
        "sessions",
        "subject_role_bindings",
        "users",
    ])
    .fetch_all(&pool)
    .await
    .expect("table lookup should succeed");

    assert_eq!(
        table_names,
        vec![
            "audit_logs",
            "oauth_authorization_codes",
            "oauth_clients",
            "oauth_consents",
            "oauth_refresh_tokens",
            "password_credentials",
            "permissions",
            "policy_rules",
            "role_permissions",
            "roles",
            "sessions",
            "subject_role_bindings",
            "users",
        ]
    );
}
