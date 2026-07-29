# Input scenarios

Input scenarios are the shared action-sequence format for trained policies,
training environments, and the Admin debug console. One entry in `frames`
always represents exactly one authoritative game tick.

```json
{
  "schema_version": 1,
  "name": "distance-keeper-episode-42",
  "frames": [
    {
      "note": "observation 0",
      "inputs": [
        {
          "player_id": 2,
          "move_x": -1.0,
          "move_y": 0.0,
          "aim_x": 1.0,
          "aim_y": 0.0,
          "shooting": true,
          "reload_pressed": false,
          "dash_pressed": false,
          "reason": "enemy is inside the preferred engagement range",
          "metadata": {
            "confidence": 0.87,
            "policy": "distance_keeper_v1"
          }
        }
      ]
    }
  ]
}
```

Omitted action fields use their neutral defaults. Movement is clamped to unit
length and aim is normalized by the Game Core. `reason` and `metadata` do not
affect simulation; they let the observer inspect policy decisions alongside the
tick where the action was applied.

## Admin API

Load a scenario and automatically pause the selected Game Server:

```text
POST /api/servers/{server_id}/input-scenario
Content-Type: application/json
```

The request body is the scenario itself. Once loaded, call:

```text
POST /api/servers/{server_id}/step
{"ticks": 1}
```

Each step consumes one frame before advancing the Game Core. The regular
snapshot endpoint is updated every tick for deterministic observation:

```text
GET /debug/api/state?server_id={server_id}
GET /api/servers/{server_id}/state
```

The server state includes `input_scenario.next_frame` and
`input_scenario.last_applied`, including policy explanations. Clear the queued
sequence with:

```text
POST /api/servers/{server_id}/input-scenario/clear
```

An input only overrides the named player for that frame. CPU players not named
in a frame continue using the built-in policy. When an override ends, its
one-shot movement and buttons are released before normal control resumes.

