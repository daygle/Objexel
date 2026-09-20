# Objexel Upgrades

Objexel uses an operator-controlled update channel. It never replaces a running installation automatically.

## Before upgrading

1. Review the GitHub release notes and image digest.
2. In the web UI, open the update notice or select **Check for updates**. This queries the latest `daygle/Objexel` GitHub release; forks can point it elsewhere with `OBJEXEL_UPDATE_REPO` (e.g. `owner/repo`).
3. Back up PostgreSQL, configuration, model metadata, and media metadata. See [backup.md](backup.md).
4. Confirm there is enough disk space for the new image, migrations, and temporary model/download files.
5. Record the current image tag and verify a rollback image is available.

## Docker Compose upgrade

Set `OBJEXEL_IMAGE_TAG` to the release you reviewed (it defaults to `latest`). Compose pulls `ghcr.io/daygle/objexel:$OBJEXEL_IMAGE_TAG`, published by the release workflow.

```sh
export OBJEXEL_IMAGE_TAG=v0.2.0   # the release you verified
docker compose pull api
docker compose up -d api
docker compose ps
curl -fsS http://localhost:8080/health
curl -fsS http://localhost:8080/liveness
curl -fsS http://localhost:8080/readiness
```

Persist `OBJEXEL_IMAGE_TAG` in your `.env` so restarts stay on the pinned release. For source-based deployments, check out the desired release tag and run `docker compose up -d --build` (this rebuilds locally and tags the image as the same reference).

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
export OBJEXEL_IMAGE_TAG=v0.1.0   # the previously verified release
docker compose up -d api
```

Set `OBJEXEL_IMAGE_TAG` to the previously verified release tag so Compose runs that image (keep it in `.env`). Do not remove PostgreSQL volumes during a rollback.

## Release process

Tagged releases use `.github/workflows/release.yml` to build and publish a versioned image to GitHub Container Registry and create GitHub release notes. Push a semantic-version tag such as `v0.2.0` only after CI, migration review, and an end-to-end camera test pass.
