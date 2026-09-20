# Objexel Backups

Back up PostgreSQL metadata and media independently. Test restores on a separate host before relying on them.

## Database backup

```sh
docker compose exec -T postgres pg_dump -U objexel -d objexel --format=custom > objexel-$(date +%F).dump
```

Restore into an empty test database:

```sh
createdb -U objexel objexel_restore
pg_restore -U objexel -d objexel_restore --clean --if-exists objexel-YYYY-MM-DD.dump
```

## Configuration and media

Keep `docker-compose.yml`, model metadata, and deployment configuration in version control, but never commit credentials. Back up the `OBJEXEL_STORAGE_DIR` recordings, clips, and snapshots directory with a filesystem tool that preserves timestamps and permissions.

A valid backup must include a dump checksum, dump size, migration version, and a manifest of media files. Validate by restoring PostgreSQL, checking `/readiness`, and opening a representative clip and snapshot.

## Recovery policy

Use a documented retention window and an off-host copy. Backups are not complete until a restore drill succeeds. Treat RTSP credentials and notification provider configuration as secrets; rotate them if a backup is exposed.
