# agent-18 · Freight carrier-call agent: latency + inference cost optimizer

**Live demo:** https://agent-18-voice-agent-latency.braylo777.workers.dev  ·  API: `POST /api/call {"utterances":[...]}` streams Server-Sent Events.

## What
A production-style inbound carrier-call agent for a freight broker, running as a streaming Cloudflare Worker:
**STT → NLU (MC number, load id, lane, rate ask) → load lookup (lane cache) → rate negotiation with guardrails → TTS stream → call summary.**

**STT and TTS are simulated with text.** Input arrives as text utterances, and the TTS stage streams 12-character text chunks in place of audio frames. No speech models are run yet.

Guardrails: never offer above the load's max rate, never expose max/target (`/api/loads` strips them), require an MC number before quoting, max 3 counter rounds, then decline.

Inference optimizer (`route()` in `src/core.js`): for each turn type it picks the **cheapest config whose measured quality and p95 latency meet the target** (default quality ≥ 0.95, p95 ≤ 800 ms). Prices are Cloudflare Workers AI's published per-M-token rates (read 2026-10-27). Configs with no measured profile are never chosen. Deterministic turns use the lane cache, and summaries are batched ×8.

## Data
60 **synthetic** carrier calls (seeded, `evals/calls.mjs`) against 40 synthetic loads. These are not real transcripts.

## Results (real, from `evals/results.json`, measured 2026-10-27)
**Per-stage CPU latency** (Node 22 on the box, 50 reps × 60 calls):

| stage | p50 ms | p95 ms | n |
|---|---|---|---|
| stt | 0.0003 | 0.0007 | 10000 |
| nlu | 0.0018 | 0.004 | 10000 |
| lookup | 0.0001 | 0.0011 | 10000 |
| negotiate | 0.0001 | 0.0003 | 10000 |
| tts_first_chunk | 0.0003 | 0.0008 | 10000 |
| summary | 0.0002 | 0.0004 | 3000 |

**Live end-to-end, deployed Worker** (curl from the box, n=40, includes network + TLS):

| metric | p50 | p95 |
|---|---|---|
| time to first streamed token | 108.3 ms | 191.3 ms |
| total call response | 108.6 ms | 191.4 ms |

**Call outcomes (60 calls):** booked 42, declined 6, no load 12. **Guardrail violations: 0.**

**Optimizer vs naive baseline** (naive = every agent turn and summary is a separate llama-3.1-70b-fast request):

| | naive | optimized |
|---|---|---|
| model requests (60 calls) | 260 | 0 |
| cost per call | $0.000287 | $0.000000 |
| routed config (dialog / summary) | 70b / 70b | template / template |
| lane lookup p50 / p95 (4,040-load board) | 0.0014 / 0.0073 ms | 0.0003 / 0.0006 ms (cache hit rate 0.9833) |
| summary requests | 60 | 8 (batched ×8) |

Caveats, stated plainly:
- Naive cost uses token counts estimated as chars/4 (no tokenizer) × published 70b prices.
- Today the only *measured* config is the deterministic template, scored quality 1.0 as its own reference. So the router picks it and optimized cost is $0. That shows the router logic, not model-vs-model savings, which need the post-reset profiling below.
- The cache hit rate is high because the synthetic calls repeat lanes.

## How to run
```bash
npm test            # 18 offline tests (guardrails, router, cache, batcher, pipeline)
npm run bench       # offline per-stage + optimizer benchmark -> evals/results.json
npm run bench:live  # live TTFT/total against the deployed Worker
npm run build       # dist/worker.js;  or: npx wrangler deploy
```
No dependencies. Free tier only. Rollback tag: `v0-rollback`. CI job in `ci/github-actions-ci.yml` (copy to `.github/workflows/` to enable).

## Pending (after the Workers AI quota reset, 7 PM CT Oct 28 2026)
- Profile llama-3.2-1b, llama-3.1-8b-fast and llama-3.1-70b-fast on the dialog and summary turns: quality vs template/gold, p50/p95, TTFT. Then rerun the router for real model-vs-model cost and latency savings.
- Replace the chars/4 estimate with the token usage the API returns.
- Optional real STT/TTS (`@cf/openai/whisper`, a TTS model) to replace the text simulation.
