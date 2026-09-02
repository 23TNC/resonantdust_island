# Work

Each subfolder here is one **unit of work**: a self-contained chunk of the
project that gets planned up front, executed, and then left in place as a
record of what happened.

## Naming

`NNNN-short-kebab-slug` — a zero-padded sequence number followed by a short
description. The number gives a stable ordering; the slug says what it is.

## Required contents

Every work folder contains at least these two files.

### `todo.md`

The plan, written *before* the work starts. It states the goal, the acceptance
criteria, and a checklist of tasks. Tasks are checked off as they are
completed, so the file doubles as live progress. If the plan changes mid-flight,
edit the plan and note why — do not silently drop tasks.

### `issues.md`

The running log of problems encountered while doing the work: build failures,
API surprises, environment quirks, wrong assumptions in the plan. Each entry
records the symptom, the cause once known, and the resolution. This file is
written *during* the work, not after.

An issue that is resolved stays in the file with its resolution attached. An
issue that is not resolved stays open at the top and is carried into the next
unit of work if it still matters.

## Status vocabulary

- `[ ]` not started
- `[~]` in progress
- `[x]` done
- `[-]` dropped (with a one-line reason)
