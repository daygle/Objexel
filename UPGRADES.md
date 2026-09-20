# Objexel Upgrades

1. Read the release notes and inspect the migration files.
2. Take and verify a PostgreSQL backup and media backup.
3. Stop or quiesce external traffic if the release requires it.
4. Pull the new image/source and run `docker compose up -d --build`.
5. Confirm container health, `/liveness`, `/readiness`, and `/metrics`.
6. Verify one camera, one detection, one event, and one media clip.

Migrations run at API startup and are applied in order by SQLx. Never edit an applied migration. If startup fails during a migration, preserve the database and logs, restore the backup only after investigation, and do not skip migration versions.

Rollback the application binary only when the database schema remains compatible. If a migration changed the schema, restore the database backup into a test environment and validate the previous release before any production rollback.
