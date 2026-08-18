# Skout Parity Checklist

Use this table as the working source of truth for hands-on skout-to-b9 command comparisons. Update a row after manually comparing complete output and behavior, including data, formatting, colors, alignment, flags, errors, and side effects.

Treat `99%` as functionally complete with minor parity defects still possible. Reserve `100%` for a fresh, complete manual comparison with no known difference. Inspect the skout implementation whenever behavior or data sourcing is unclear.

| b9 command | Workflow | Parity | State | Current focus or known gap |
|---|---|---:|---|---|
| root help / version | Command discovery and version output | — | Not assessed | Compare retained commands, flags, streams, and version forms; Yahoo OAuth surfaces intentionally diverge from skout. |
| `logout` | Retired Yahoo credential cleanup | — | Transitional | Verify exact deletion, missing-credential behavior, and removal after one released cleanup window. |
| `st` | Status and league selection | — | Not assessed | Compare dashboard fields, colors, league selection, freshness, and daemon state. |
| `sync` | Complete foreground synchronization and public Yahoo setup | 99% | Functionally complete | Continue watching unofficial endpoint availability, snapshot completeness, team selection, retained stale data, progress output, and runtime. |
| `start` | Start background synchronization | — | Not assessed | Compare already-running, stale-state, process, log, and schedule behavior. |
| `stop` | Stop background synchronization | — | Not assessed | Compare stopped, stale-process, timeout, cleanup, and output behavior. |
| `restart` | Restart background synchronization | — | Not assessed | Compare composed stop/start behavior and failure recovery. |
| `log` | Read or follow daemon logs | — | Not assessed | Compare default tail, line count, follow, truncation, path, and missing-log behavior. |
| `reset` | Remove local b9 state | — | Not assessed | Compare confirmation, cancellation, daemon interaction, deletion scope, and output. |
| `lm` | Advisory provider and model configuration | — | Not assessed | Compare provider selection, model discovery, credentials, cancellation, and errors. |
| `m` | Daily or weekly fantasy matchup | 99% | Functionally complete | Continue logging minor live-data, matchup-total, status, odds, color, and alignment discrepancies. |
| `t` | MLB 40-man roster display | — | Not assessed | Compare roster membership, role/status classification, columns, sorting, and colors. |
| `tt` | MLB standings and team totals | — | Not assessed | Compare standings, totals, Yahoo-player counts, sorting, formatting, and freshness. |
| `sp` | Probable-pitcher slate and odds | — | Not assessed | Compare dates, starters, ownership, odds sources, degradation, sorting, and colors. |
| `r` | Fantasy roster display | 99% | Functionally complete | Continue logging minor live-data, status, color, position, and alignment discrepancies. |
| `rt` | Fantasy roster totals | 99% | Functionally complete | Compare `-w/--weekly` for the current week, an explicit week number, and an ISO date; continue logging minor live-data, aggregation, color, and alignment discrepancies. |
| `h` | Hitter pool and hitter detail | — | Not assessed | Compare default list, sorting, position and waiver filters, detail view, data, and colors. |
| `p` | Pitcher pool and pitcher detail | — | Not assessed | Compare default list, sorting, position and waiver filters, detail view, data, and colors. |
| `i` | Glossary lookup | — | Not assessed | Compare full glossary, exact lookup, ambiguity, suggestions, selection, and formatting. |
| `help` | Command help dispatch | — | Not assessed | Compare command routing, command-specific text, unknown commands, streams, and exits. |

## Historical References

These fixed-baseline inventories remain useful as source maps. They are frozen historical evidence, not current parity-status documents.

- [`skout-parity.md`](skout-parity.md) — source baseline, evidence policy, capability taxonomy, and coverage manifest.
- [`skout-cli-operations.md`](skout-cli-operations.md) — command and operational behavior inventory.
- [`skout-providers-storage.md`](skout-providers-storage.md) — provider, cache, persistence, freshness, and synchronization inventory.
- [`skout-analysis-display-advisory.md`](skout-analysis-display-advisory.md) — analysis, display, advisory, and readiness inventory.
