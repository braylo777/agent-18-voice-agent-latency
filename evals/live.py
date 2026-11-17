"""Live end-to-end benchmark against the deployed Worker: time to first byte (first streamed token) and total, N=40 calls.
Usage: URL=https://.../api/call N=40 python3 evals/live.py"""

import json, os, time, urllib.request

URL = os.environ.get(
    "URL", "https://agent-18-voice-agent-latency.braylo777.workers.dev/api/call"
)
N = int(os.environ.get("N", "40"))
BODY = json.dumps(
    {
        "utterances": [
            "Hey, calling on L1000, MC 123456",
            "What is it paying?",
            "I need $3000 all in",
            "Can you do $2900",
        ]
    }
).encode()
for _ in range(N):
    t0 = time.perf_counter()
    req = urllib.request.Request(URL, BODY, {"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        r.read(1)
        ttfb = time.perf_counter() - t0
        r.read()
    print(f"{ttfb:.6f} {time.perf_counter() - t0:.6f}")
