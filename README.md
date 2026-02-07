# cronclaw

Cron-driven pipeline orchestrator for agents and programs.

cronclaw runs stateful, step-by-step pipelines where each invocation advances exactly one step. The OS cron handles the loop — cronclaw just does one tick and exits.

Designed to work with [OpenClaw](https://github.com/open-claw) agents, but works with any bash command.

## Install

```
cargo install --path .
```

## Usage

```bash
cronclaw init                 # set up ~/.cronclaw/
cronclaw run                  # advance pipelines by one step
cronclaw reset <pipeline>     # restart a pipeline
```

Then point cron at `cronclaw run` at whatever interval you want.

## Pipelines

Create a directory under `~/.cronclaw/pipelines/` with a `pipeline.yaml`:

```yaml
version: 1
workspace: workspace

steps:
  - id: fetch-data
    type: bash
    bash: curl -o data.json.tmp https://api.example.com/data
    outputs:
      - name: data
        path: data.json
        tmp: data.json.tmp

  - id: analyse
    type: agent
    agent: pro-worker
    prompt: |
      Analyse the following data and write a summary:
      {{ file:data.json }}
```

### Step types

**bash** — runs a shell command in the workspace directory.

**agent** — spawns an OpenClaw agent with a prompt. Prompts support `{{ file:path }}` to inject file contents from the workspace.

### Outputs

Steps can declare outputs with a `tmp` and final `path`. The tmp file is promoted to the final path only on success, so downstream steps never see partial results.

### State

Each step tracks its own status: `pending`, `running`, `completed`, or `failed`. State is stored in `state.json` next to the pipeline. Missing state file means the pipeline starts fresh on the next tick.
