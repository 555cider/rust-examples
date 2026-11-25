do
$$
begin
    if not exists (select 1 from pg_roles where rolname = 'auth_server') then
        create role auth_server with login password 'auth_server' createdb;
    end if;
end
$$;

select 'create database auth_server'
where not exists (select 1 from pg_database where datname = 'auth_server')\gexec

alter database auth_server owner to auth_server;
grant all privileges on database auth_server to auth_server;
