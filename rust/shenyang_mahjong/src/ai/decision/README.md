# Shenyang Mahjong AI Decision Layout

This directory is split by decision responsibility. Keep `mod.rs` as a thin
entry point and put new rule or heuristic code in the smallest matching module.

## Directory tree

```text
decision/
  mod.rs              # module wiring and public exports only
  claim/              # Hu/Gang/Peng/Chi/Pass choices while reacting to a discard
    peng/             # Peng-specific offensive, defensive, and route-preserving heuristics
  defense/            # public danger reads and defensive discard/opening logic
  discard/            # own-turn discard orchestration and discard scoring adjustments
  hand/               # closed-hand shape, suit, pair, sequence, and requirement helpers
  piao/               # piao-hu planning, waits, and discard choices
  score/              # progress, readiness, fan, and pressure scoring
  seven_pairs/        # seven-pairs planning, waits, and discard choices
  shenyang_rule/      # Shenyang-basic progress and requirement guards
  tests/              # tests mirroring the production decision layout
```

## Entry points

- `mod.rs`: module wiring and public exports only.
- `claim/mod.rs`: claim-window orchestration for Hu, Gang, Peng, Chi, and Pass.
- `discard/mod.rs`: discard orchestration for the current hand.
- `self_gang.rs`: concealed and added-gang decisions on the AI player's turn.

## Shared evaluators

- `hand/`: closed-hand shape helpers, suit counts, pair/triplet checks, and
  tile removal helpers.
- `meld.rs`: meld validation, meld construction, and meld shape helpers.
- `score/`: readiness, visible fan, pressure, and progress scoring.
- `table.rs`: public table reads, visible counts, remaining tiles, and
  simulated discard visibility.
- `tile.rs`: tile-kind predicates and tile identity helpers.
- `types.rs`: small decision types shared by callers.

## Strategy modules

- `claim/`: claim-window decisions.
- `claim/peng/`: Peng-specific heuristics used by `claim/peng_choice.rs`.
- `defense/`: danger and defensive-open heuristics.
- `piao/`: Piao-hu planning and discard helpers.
- `seven_pairs/`: seven-pairs planning, waits, and discard choices.
- `pure_one_suit.rs`: pure-one-suit planning.
- `shenyang_rule/`: Shenyang-basic rule progress, recovery checks, and discard
  requirement guards.
- `round.rs`: round-phase helpers.

## Tests

- `tests.rs` contains shared test helpers and wires submodules.
- `tests/` mirrors the production decision layout where practical.

Prefer adding a new focused module when a file starts mixing unrelated
heuristics. Avoid putting new business logic directly into `mod.rs`.
As a rough limit, production decision files should stay comfortably below a few
hundred lines; split by rule, route, or table-read responsibility before a file
starts collecting unrelated heuristics.
