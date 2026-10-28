#!/usr/bin/env bash
# Live end-to-end benchmark against the deployed Worker: time to first byte (first streamed token) and total, N=40 calls.
URL=${URL:-https://agent-18-voice-agent-latency.braylo777.workers.dev/api/call}
for i in $(seq 1 ${N:-40}); do curl -s -o /dev/null -w "%{time_starttransfer} %{time_total}\n" -X POST "$URL" -d '{"utterances":["Hey, calling on L1000, MC 123456","What is it paying?","I need $3000 all in","Can you do $2900"]}'; done
