# cronclaw Architecture

## The Problem

AI agents are good at complex reasoning and multi-step tasks. But they're terrible at *waiting*. If you need an agent to wake a machine over WOL, wait 3 minutes for it to boot, then SSH in and do work — a single agent prompt falls apart. Agents don't wait. They hallucinate, retry, or time out.

The real-world pattern is: many workflows are a **mix of agentic tasks and programmatic steps**, and some of those programmatic steps involve waiting. You need to chain them together — agent does analysis, script sends a request, wait for a server, agent processes the result, script deploys it.

cronclaw solves this by splitting these workflows into discrete steps and letting `cron` (or any scheduler) drive execution. Each invocation does exactly one thing: run the next pending step. The scheduler provides the loop and the waiting.

## Core Invariant

**One invocation = one step.** `cronclaw run` finds the next pending step, executes it, updates state, and exits. There is no internal loop. Every "tick" is a fresh process invocation. This means waiting between steps is free — it's just time between cron runs.

## How It Works

A pipeline is a YAML file defining an ordered list of steps. Each step is either `bash` (run a shell command) or `agent` (invoke an AI agent with a prompt). State is tracked in a JSON file alongside the pipeline.

```
cronclaw run   (tick 1) → executes step 1 → exits
cronclaw run   (tick 2) → executes step 2 → exits
cronclaw run   (tick 3) → executes step 3 → exits, pipeline complete
```

If a step is still running (process crashed mid-execution), the next invocation sees `running` status and exits — no double-execution. If a step failed, the pipeline is blocked until manually reset with `cronclaw reset`.

## Pipeline Definition

```yaml
version: 1
workspace: workspace
steps:
  - id: wake-server
    type: bash
    bash: wol AA:BB:CC:DD:EE:FF && sleep 180

  - id: gather-data
    type: bash
    bash: ssh server 'dump-stats' > data.json.tmp
    outputs:
      - name: data
        tmp: data.json.tmp
        path: data.json

  - id: analyse
    type: agent
    agent: pro-worker
    prompt: |
      Analyse this data and write recommendations:
      {{ file:data.json }}
```

Steps execute sequentially in definition order. `{{ file:path }}` templates inject workspace file contents into agent prompts. Outputs use tmp/final promotion — a step writes to `data.json.tmp`, and only on success does cronclaw rename it to `data.json`, so downstream steps never see partial results.

## Project Structure

```
src/
  main.rs       CLI entry point (init, run, reset commands)
  pipeline.rs   YAML parsing and validation of pipeline definitions
  state.rs      State persistence (pending/running/completed/failed per step)
  runner.rs     Execution engine — step dispatch, timeouts, template resolution, output promotion
  config.rs     Global config loading (just timeout default for now)
  lib.rs        Re-exports modules for integration tests
```

## Runtime Layout

```
~/.cronclaw/
  config.yaml
  pipelines/
    my-pipeline/
      pipeline.yaml             # the pipeline definition
      state.json                # auto-managed execution state
      state.lock                  # transient lock file (held only during state transitions)
      workspace/                # working directory for steps
```

## State Machine

Each step has one of four statuses:

```
Pending ──run──► Running ──success──► Completed
                    │
                    └──failure──► Failed (pipeline blocked)
```

State is saved to disk *before* execution (marking `running`) and *after* (marking `completed` or `failed`). This means a crash mid-step leaves the state as `running`, and the next invocation exits cleanly rather than re-executing.

There is no automatic retry. `Failed` means "human, look at this." Reset with `cronclaw reset <pipeline>` to start over.

## Key Design Decisions

**Why external loop (cron) instead of an internal loop?** Because the whole point is enabling workflows that span minutes or hours between steps. An internal loop would need to sleep, handle signals, manage its own scheduling. Cron already does all of that.

**Why lock state.json during transitions?** When a process reads `pending` and decides to run a step, there's a brief window before it writes `running` to disk. A concurrent process could read the same `pending` in that window. cronclaw locks `state.lock` exclusively during this read-decide-write transition, then releases it immediately. The lock is *not* held during step execution — only during the microseconds it takes to claim the step. This means concurrent invocations against the same pipeline are safe at any frequency, while long-running steps don't block other processes from checking state.

**Why state on disk?** No database, no daemon, no dependencies. A pipeline is a directory with two files. You can inspect, edit, or reset state with basic tools.

**Why tmp/final output promotion?** Atomicity. A step that writes `data.json.tmp` and then crashes won't leave a corrupt `data.json` for the next step to consume.

## Running Tests

```bash
cargo test
```

Tests use temp directories for full isolation. Key test scenarios: template resolution, output promotion, multi-step tick advancement, failure blocking, state mismatch detection, and running-step crash resilience.
