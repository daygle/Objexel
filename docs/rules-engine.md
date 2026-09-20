# Phase 6 rules engine

`objexel-rules` is the decision layer after observations. It evaluates all enabled rules against an observation context containing object class, zone, confidence, and duration. Every condition on a rule must match before an event is generated.

Supported conditions:

- object class
- zone ID
- observation type (`zone_entered`, `zone_exited`, `presence`, etc.)
- minimum confidence
- minimum duration

Each rule has independent cooldown and suppression windows. The in-process engine keeps the most recent emission per rule and suppresses duplicate events until the larger configured window expires. Generated events are persisted in PostgreSQL and contain rule, camera, track, observation, severity, summary, and timestamp references.

## APIs

- `GET /api/rules`
- `GET /api/rules/{id}`
- `POST /api/rules`
- `PUT /api/rules/{id}`
- `DELETE /api/rules/{id}`
- `GET /api/events`
- `GET /api/events/{id}`

The Rules and Events views are available at `/rules` and `/events`. Notifications, recordings, and automation actions are intentionally deferred.
