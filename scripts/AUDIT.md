# Pipeline timing audit

Run from the repository root:

```bash
nix develop -c node scripts/audit.mjs --url 'https://your-job-posting' --out /tmp/applytool-audit
nix develop -c node scripts/audit-local.mjs /tmp/applytool-audit
```

The default model is `opencode-go/gpt-6-luna`. Override it with `--model provider/model`.
`--data` overrides the default `~/ApplyTool` templates, skills, and fonts directory.
Omitting `--url` serves a synthetic HR posting locally; its HTTP timing is not representative of a real job board.
Use a new output directory for each run.

The audit uses the production prompt builders and mirrors extraction, file-block parsing,
English generation, sequential PDF/DOCX/ODT/PNG exports, the two-round repair budget,
and German translation. Each AI task starts a fresh OpenCode process. Only the JSON
output format differs from the app's default OpenCode command, to collect event and
token statistics. CLI/model configuration remains inherited, just as in the app.
Processes have a ten-minute limit, which the app currently lacks.

`report.json` contains individual timings, first observed event times, provider token
counts, tool calls, and warnings. `milestones.json` contains time to English availability.
Prompts, CLI events, Typst files, exports, and previews stay in the output directory.
These artifacts include CV/application content. Application settings and jobs are not modified.

The supplementary local audit builds a temporary SQLite database from the app's schema
and current company list. It measures equivalent SQL work and company normalization in
JavaScript, not the actual Rust implementation. `local-report.json` reports those times
separately; they are not part of the main run's total. It tests a new one-job database,
so list timings do not represent a large existing job history.

This is a backend CLI audit, not a desktop integration test. It excludes Tauri IPC,
Svelte state updates/paint, screenshot handling, editor live preview, and reopen rendering.
AI event times cannot isolate network wait, provider queueing, thinking, or token generation.
Reasoning token counts are reported by OpenCode; no reasoning duration is inferred from them.
One run is a sample, not a stable latency benchmark; repeat to assess variation.
The test fails on missing requested files, invalid JSON, unsuccessful PDF compilation,
or unresolved letter overflow. Pandoc failures are recorded as warnings, matching the app.
