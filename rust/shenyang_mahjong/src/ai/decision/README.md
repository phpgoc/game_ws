# Shenyang Mahjong AI Decision Layout

The old monolithic `decision.rs` has been split into this directory. Keep
`mod.rs` as a thin entry point and put new rule or heuristic code in the
smallest matching module.

## Directory tree

```text
decision/
  mod.rs              # module wiring and public exports only
  claim/              # Hu/Gang/Peng/Chi/Pass choices while reacting to a discard
    peng/             # Peng-specific offensive, defensive, and route-preserving helpers
  defense/            # public danger reads and defensive discard/opening logic
  discard/            # own-turn discard orchestration and discard scoring adjustments
  hand/               # closed-hand shape, suit, pair, sequence, and requirement helpers
  meld.rs             # meld validation, construction, and shape helpers
  piao/               # piao-hu planning, waits, defense, and discard choices
  pure_one_suit.rs    # pure-one-suit planning
  round.rs            # round-phase helpers
  score/              # progress, readiness, fan, and pressure scoring
  self_gang.rs        # concealed and added-gang choices on own turn
  seven_pairs/        # seven-pairs planning, waits, and discard choices
  shenyang_rule/      # Shenyang rule progress and requirement guards
  table/              # public table reads, visibility, remaining tiles, and turn order
  tile.rs             # tile predicates and identity helpers
  types.rs            # small shared decision types
  tests/              # tests mirroring the production decision layout
    claim/
      peng/
    defense/
    piao/
```

## Entry points

- `mod.rs`: module wiring and public exports only.
- `claim/mod.rs`: claim-window orchestration for Hu, Gang, Peng, Chi, and Pass.
- `discard/mod.rs`: discard orchestration for the current hand.
- `self_gang.rs`: concealed and added-gang decisions on the AI player's turn.

Do not put business heuristics directly in `mod.rs`. If a helper is only used
by one decision path, place it under that path instead of exporting it from the
root.

## Shared evaluators

- `hand/`: closed-hand shape helpers, suit counts, pair/triplet checks, and
  tile removal helpers.
- `meld.rs`: meld validation, meld construction, and meld shape helpers.
- `score/`: readiness, visible fan, pressure, and progress scoring.
- `table/`: public table reads, visible counts, remaining tile projections,
  turn-order helpers, and simulated discard visibility.
- `tile.rs`: tile-kind predicates and tile identity helpers.
- `types.rs`: small decision types shared by callers.

## Strategy modules

- `claim/`: claim-window decisions.
- `claim/peng/`: Peng-specific heuristics used by `claim/peng_choice.rs`.
- `defense/`: danger and defensive-open heuristics.
- `piao/`: Piao-hu planning and discard helpers.
- `seven_pairs/`: seven-pairs planning, waits, and discard choices.
- `pure_one_suit.rs`: pure-one-suit planning.
- `shenyang_rule/`: Shenyang rule progress, recovery checks, and discard
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

When adding tests for a split production module, prefer the same path under
`tests/` where practical. For example:

- `claim/peng/basic.rs` -> `tests/claim/peng/basic.rs`
- `defense/pure_threat.rs` -> `tests/defense/pure_threat.rs`
- `piao/plan.rs` -> `tests/piao/plan.rs`
