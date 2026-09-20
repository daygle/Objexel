# Phase 5 spatial zones

Zones use normalized coordinates (`0.0` to `1.0`) relative to a camera frame. A zone requires at least three points and is evaluated with a ray-casting point-in-polygon test against the center of each tracked bounding box.

`objexel-zones` keeps short-lived occupancy state in memory for fast multi-zone/multi-track evaluation. PostgreSQL stores the durable zone definition and every entered/exited event. The pipeline loads enabled zones for the frame camera, evaluates all active tracks, persists events, and emits corresponding observations.

## APIs

- `GET /api/zones`
- `GET /api/zones/{id}`
- `POST /api/zones`
- `PUT /api/zones/{id}`
- `DELETE /api/zones/{id}`
- `GET /api/zone-events`

The zone editor is available at `/zones`. It draws normalized polygon vertices over the camera snapshot endpoint and supports labels, colors, and saving zone definitions.

Zone events are intentionally the foundation for future rules and actions. Notifications, automation, and recordings are not part of this phase.
