# Live Detection Path — Complete Trace

## Verdict: **FULLY CONNECTED** ✅

Every step from RTSP stream to action dispatch is wired and executes during
normal operation. There are no gaps, dead ends, or disconnected stages.

---

## Step-by-Step Trace

### Step 1: RTSP Stream

| | |
|---|---|
| **What** | FFmpeg subprocess connects to the RTSP URL and begins decoding |
| **File** | `crates/camera/src/lib.rs` |
| **Method** | `run_stream()` (line ~210) |
| **Called by** | `FrameIngestor::spawn()` → spawned `tokio::spawn` task |
| **Spawned by** | `crates/api/src/main.rs` — live frame ingestion block (line ~105) |
| **How** | `Command::new(&config.ffmpeg_bin).args(["-rtsp_transport", "tcp", "-i", rtsp_url, ...]).spawn()` |
| **Reconnect** | Exponential backoff (2s → 60s) on any error or clean EOF |

**Connection:** ✅ Spawned in `main.rs` for every enabled camera after models are loaded.

---

### Step 2: Frame Decode

| | |
|---|---|
| **What** | FFmpeg outputs raw RGB24 frames to stdout at the configured FPS |
| **File** | `crates/camera/src/lib.rs` |
| **Method** | `run_stream()` → `stdout.read_exact(&mut frame_buf).await` (line ~235) |
| **Called by** | `run_stream()` (same function, async read loop) |
| **How** | FFmpeg args: `-vf fps={N} -f rawvideo -pix_fmt rgb24 pipe:1`. Each frame is `width × height × 3` bytes. `read_exact` blocks until a complete frame is available. |
| **FPS control** | FFmpeg's `-vf fps=N` filter drops frames to match target. Default: 5 fps (`OBJEXEL_INGEST_FPS` env var). |

**Connection:** ✅ `read_exact` loop feeds each complete frame to the callback.

---

### Step 3: FrameTensor Creation

| | |
|---|---|
| **What** | `DecodedFrame` (raw RGB24 bytes) is converted to `FrameTensor` (NCHW float32) |
| **File** | `crates/api/src/main.rs` |
| **Method** | Inline async closure in `ingestor.spawn()` callback (line ~115) |
| **Called by** | `run_stream()` via the `on_frame` callback |
| **How** | `FrameTensor { camera_id, observed_at: Utc::now(), shape: [1, 3, H, W], data: pixels.map(\|p\| p as f32 / 255.0) }` |
| **Normalization** | Pixel values `[0, 255]` → `[0.0, 1.0]` float |

**Connection:** ✅ Closure is passed directly to `FrameIngestor::spawn()`, invoked for every decoded frame.

---

### Step 4: ObservationPipeline::process_frame()

| | |
|---|---|
| **What** | The full pipeline is invoked with the `FrameTensor` |
| **File** | `crates/pipeline/src/lib.rs` |
| **Method** | `ObservationPipeline::process_frame()` (line ~35) |
| **Called by** | The callback closure in `main.rs`: `pipeline.process_frame(tensor).await` (line ~120) |
| **Returns** | `PipelineResult { detections, tracks, observations, zone_events, events }` |

**Connection:** ✅ Called directly from the ingestor callback. Every frame enters the pipeline.

---

### Step 5: Detector (ONNX Inference)

| | |
|---|---|
| **What** | `FrameTensor` is fed through an ONNX model to produce `Vec<Detection>` |
| **File** | `crates/detector/src/lib.rs` |
| **Method** | `ModelManager::detect_with_model()` (line ~85) |
| **Called by** | `process_frame()` at lines 37-43 (single model) or 40-42 (multi-model via `tokio::spawn` per assignment) |
| **How** | `ArrayD::from_shape_vec()` → `session.run(TensorRef)` → parses 6-value chunks `[x, y, w, h, confidence, class_id]` |
| **Multi-model** | Each model assignment runs as a separate `tokio::spawn` job for parallelism |

**Connection:** ✅ Called for every frame. If no models are loaded, returns empty detections (no crash).

---

### Step 6: Tracker

| | |
|---|---|
| **What** | Detections are matched to existing tracks or create new tracks |
| **File** | `crates/tracker/src/lib.rs` |
| **Method** | `Tracker::update()` (line ~25) |
| **Called by** | `process_frame()` at line 49: `self.camera_tracker(camera_id).await.lock().await.update(&mut detections, now)` |
| **How** | For each detection: finds best IoU match among tracks with same `camera_id` + `object_class` within `max_age`. Assigns `track_id` to detection. Returns matched `Vec<Track>`. |
| **Camera scoping** | Per-camera `Tracker` instances — Camera A tracks never contaminate Camera B |

**Connection:** ✅ Called for every frame. Tracker state persists across frames (track continuity).

---

### Step 7: Observations

| | |
|---|---|
| **What** | Tracks that exceed the minimum duration threshold become observations |
| **File** | `crates/observations/src/lib.rs` |
| **Method** | `ObservationEngine::from_track()` (line ~18) |
| **Called by** | `process_frame()` at line 56: `tracks.iter().filter_map(\|track\| self.observations.from_track(track))` |
| **Threshold** | Default: 3 seconds (`minimum_duration_ms: 3_000`). Tracks shorter than this produce no observation. |
| **Also** | Zone events (enter/exit/occupied) generate additional observations at lines 82-89 |

**Connection:** ✅ Called for every frame. Observations are the input to rule evaluation.

---

### Step 8: Rules

| | |
|---|---|
| **What** | Observations are matched against configured rules to produce events |
| **File** | `crates/rules/src/lib.rs` |
| **Method** | `RuleEngine::evaluate()` (line ~24) |
| **Called by** | `process_frame()` at line 113 (inside Phase 1 block): `rule_engine.evaluate(&rules, context, now)` |
| **How** | Each rule's conditions (object_class, zone_id, observation_type, behaviour_type, confidence_threshold, minimum_duration_ms) are checked. Cooldown/suppression prevents duplicate events. |
| **Lock scope** | Rule lock held only during `evaluate()` — DB operations happen in Phase 2 after lock release |

**Connection:** ✅ Called for every observation. Rules are loaded from DB at line 101.

---

### Step 9: Events

| | |
|---|---|
| **What** | Matched rules produce `Event` records that are persisted to the database |
| **File** | `crates/pipeline/src/lib.rs` |
| **Method** | `database.insert_event()` at line 128 |
| **Called by** | Phase 2 of `process_frame()`, iterating `rule_contexts` |
| **Also persisted** | `database.insert_notification()` (line 131), `database.insert_adaptive_scores()` (line 130) |
| **Side effects** | Event clip + snapshot spawned async (lines 133-148), not blocking the pipeline |

**Connection:** ✅ Events are created by rule evaluation and immediately persisted.

---

### Step 10: Actions

| | |
|---|---|
| **What** | Actions configured for each triggered rule are dispatched |
| **File** | `crates/actions/src/lib.rs` |
| **Method** | `ActionDispatcher::execute()` (line 14) |
| **Called by** | `process_frame()` at line 151: `self.actions.execute(&action, provider, template, event).await` |
| **How** | Looks up actions linked to the rule (`database.actions_for_rule()`), resolves provider + template, dispatches by `action_type` (email, webhook, mqtt, internal) |
| **Execution record** | `database.insert_execution()` (line 153) records success/failure |

**Connection:** ✅ Actions are dispatched for every event, for every linked action.

---

## Complete Call Graph

```
main.rs
  └─ FrameIngestor::spawn(camera_id, rtsp_url, callback)      [camera/src/lib.rs]
       └─ run_stream()                                          [camera/src/lib.rs]
            ├─ probe_dimensions()                               [camera/src/lib.rs]
            ├─ Command::new("ffmpeg").spawn()                   [camera/src/lib.rs]
            └─ stdout.read_exact() loop                         [camera/src/lib.rs]
                 └─ callback(DecodedFrame)                      [main.rs inline]
                      ├─ FrameTensor { ... }                    [main.rs inline]
                      └─ pipeline.process_frame(tensor)         [pipeline/src/lib.rs]
                           ├─ database.list_model_assignments() [database/src/lib.rs]
                           ├─ models.detect_with_model()        [detector/src/lib.rs]
                           │    └─ session.run(tensor)          [detector/src/lib.rs — ONNX]
                           ├─ fusion.fuse()                     [fusion/src/lib.rs]
                           ├─ tracker.update()                  [tracker/src/lib.rs]
                           ├─ database.insert_detection()       [database/src/lib.rs]
                           ├─ database.upsert_track()           [database/src/lib.rs]
                           ├─ behaviour.analyze()               [behaviour/src/lib.rs]
                           ├─ observations.from_track()         [observations/src/lib.rs]
                           ├─ zones.evaluate()                  [zones/src/lib.rs]
                           ├─ rule_engine.evaluate()            [rules/src/lib.rs]
                           ├─ database.insert_event()           [database/src/lib.rs]
                           ├─ actions.execute()                 [actions/src/lib.rs]
                           │    └─ dispatch by action_type      [actions/src/lib.rs]
                           └─ return PipelineResult
```

---

## Prerequisites for the Chain to Execute

| Requirement | Check |
|---|---|
| `DATABASE_URL` set | Without it, pipeline is `None` and no ingestors spawn |
| At least one enabled camera in DB | `database.list_cameras()` filters by `camera.enabled` |
| At least one ONNX model loaded | Without models, `detect_with_model` returns error (logged, frame skipped) |
| FFmpeg installed | `FrameIngestor` spawns `ffmpeg` subprocess |
| RTSP stream reachable | `probe_dimensions()` verifies connectivity before decode begins |
| Rules configured in DB | Without rules, `rule_engine.evaluate()` returns empty — no events/actions |

---

## Failure Modes

| Failure | Recovery |
|---|---|
| RTSP disconnect | `FrameIngestor` reconnects with exponential backoff (2s → 60s) |
| FFmpeg crash | `run_stream` returns error → reconnect loop retries |
| No model loaded | `detect_with_model` returns error → logged, frame skipped, next frame tried |
| DB down | `process_frame` returns error → logged, ingestor continues to next frame |
| Rule lock contention | Minimal — lock held <1ms during `evaluate()` only |
