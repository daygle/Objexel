# Objexel Upgrades

Objexel uses an operator-controlled update channel. It never replaces a running installation automatically.

## Before upgrading

1. Review the GitHub release notes and image digest.
2. In the web UI, open the update notice or select **Check for updates**.
3. Back up PostgreSQL, configuration, model metadata, and media metadata. See [BACKUP.md](BACKUP.md).
4. Confirm there is enough disk space for the new image, migrations, and temporary model/download files.
5. Record the current image tag and verify a rollback image is available.

## Docker Compose upgrade

```sh
docker compose pull api
docker compose up -d api
docker compose ps
curl -fsS http://localhost:8080/health
curl -fsS http://localhost:8080/liveness
curl -fsS http://localhost:8080/readiness
```

For source-based deployments, check out the desired release tag and run `docker compose up -d --build`.

SQLx migrations run at API startup in order. Never edit an applied migration or skip a version. If a migration fails, preserve logs and the database; investigate or restore into a test environment before production recovery.

## After upgrading

Verify:

- `/health`, `/liveness`, `/readiness`, and `/metrics`
- administrator login and user access
- one camera connection and snapshot
- one enabled ONNX model and one detection
- one event, notification, and recording clip
- model catalog/download status if model management is being used

## Rollback

Application rollback is safe only when the previous binary supports the already-applied schema. For incompatible schema changes, restore the database backup into a test environment, validate the previous release, and plan a controlled migration rollback rather than deleting the production volume.

```sh
docker compose down
docker compose up -d api
```

The command above assumes the Compose file is configured to use the previously verified image. Do not remove PostgreSQL volumes during a rollback.

## Release process

Tagged releases use `.github/workflows/release.yml` to build and publish a versioned image to GitHub Container Registry and create GitHub release notes. Push a semantic-version tag such as `v0.2.0` only after CI, migration review, and an end-to-end camera test pass.
