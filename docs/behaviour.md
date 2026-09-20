# Behavioural Intelligence

Phase 11 classifies persistent tracks inside the Rust monolith after tracking and before rule evaluation.

## Pipeline

`detection -> tracking -> observation -> behaviour -> rules -> events`

`objexel-behaviour` is deliberately deterministic and dependency-light. It evaluates dwell duration, path length, and normalized bounding-box displacement:

- **Loitering**: a track remains visible for at least 60 seconds.
- **Stationary object**: a track remains visible for at least 30 seconds with less than 5% normalized displacement.
- **Route following**: a track has at least five movement samples and meaningful displacement.

The classifier emits a confidence score and a time-bounded summary. Records are persisted in `behaviours` and can link to recordings or event clips.

## Rules

Rule conditions support `behaviour_type` alongside object, zone, observation, confidence, and duration conditions. For example:

```json
{
  "object_class": "cat",
  "behaviour_type": "loitering"
}
```

A later phase can add historical visit aggregation for repeated visits, zone hopping, and returning-object identity across tracks without changing the public behaviour record or rule interface.

## API

- `GET /api/behaviours`
- `GET /api/behaviours/{id}`
- `GET /api/search/behaviours?behaviour_type=loitering`

The `/behaviours` web page shows confidence, duration, timestamps, and links to associated recording clips.
