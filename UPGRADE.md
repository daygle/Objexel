# Objexel Upgrades

1. Read release notes and inspect new migrations.
2. Run a verified PostgreSQL and media backup.
3. Check disk space and stop external traffic if needed.
4. Pull the release and run `docker compose up -d --build`.
5. Confirm `/health`, `/liveness`, `/readiness`, and `/metrics`.
6. Verify login, one camera, one model, one detection, one event, and one clip.

SQLx migrations run at API startup in order. Never edit an applied migration or skip a version. If a migration fails, preserve logs and the database; investigate or restore into a test environment before production recovery.

Application rollback is safe only when the previous binary supports the already-applied schema. For incompatible schema changes, restore the database backup into a test environment, validate the previous release, and plan a controlled migration rollback rather than deleting the production volume.
