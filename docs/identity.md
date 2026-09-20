# Object Identity and Familiarity

Phase 13 adds a lightweight, self-hosted identity layer after tracking and before rule evaluation.

## Matching

Each track produces a compact signature from object class, average bounding-box dimensions, and normalized movement displacement. Candidate identities are restricted to the same object class and matched with a weighted similarity score. A new identity is created when no candidate reaches the configured threshold.

Identity sightings are persisted in `identity_observations`; aggregate counters and scores are maintained on `identities` and `identity_statistics`.

Familiarity categories are deterministic:

- `new`: first sighting
- `familiar`: 2–5 sightings
- `frequent`: 6–19 sightings
- `very_frequent`: 20 or more sightings

## API

- `GET /api/identities`
- `GET /api/identities/{id}`
- `PUT /api/identities/{id}` with `{ "display_name": "Neighbour Cat" }`
- `GET /api/identities/{id}/history`

Identity context is available to the rules pipeline through the assigned identity and familiarity category. Existing tracking remains the source of truth and no external recognition service is required.
