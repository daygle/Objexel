# Objexel Backup and Restore

Back up three layers independently: PostgreSQL, deployment configuration, and media. Never back up secrets into a public repository.

## Database

```sh
mkdir -p /opt/objexel/backups
cd /opt/objexel
docker compose exec -T postgres pg_dump -U objexel -d objexel --format=custom > backups/objexel-$(date +%F).dump
sha256sum backups/*.dump > backups/SHA256SUMS
```

Restore into a test database first:

```sh
createdb -U objexel objexel_restore
pg_restore -U objexel -d objexel_restore --clean --if-exists backups/objexel-YYYY-MM-DD.dump
```

## Configuration and media

Back up `docker-compose.yml`, model manifests, and the non-secret deployment configuration. Copy `recordings`, `clips`, and `snapshots` to off-host storage while preserving timestamps and permissions. Keep at least one backup offline or on a separate host.

## Validation and disaster recovery

A backup is valid only after checksum verification, PostgreSQL restore, migration/readiness checks, and opening a representative clip and snapshot. For disaster recovery, provision Debian 13 or a Proxmox VM, install Objexel from `installation.md`, restore PostgreSQL, restore media, verify `/liveness`, `/readiness`, `/metrics`, then test one camera and one event. Rotate passwords and provider credentials if the original host was compromised.

## Configuration export/import

Export deployment files and a redacted configuration manifest into the backup directory. Do not export password hashes or session tokens through a user-facing API. Recreate secrets through the deployment environment and recreate the administrator if the user database cannot be restored.
