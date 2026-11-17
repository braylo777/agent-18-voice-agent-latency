"""Freight carrier-call agent pipeline: STT(sim) -> NLU -> load lookup -> negotiation (guardrails) -> summary -> TTS(sim).
STT and TTS are SIMULATED with text: input arrives as text, output streams as text chunks."""
import math, re, time

A = re.ASCII
_WS = re.compile("[\\t\\n\\v\\f\\r \\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000\\ufeff]+")
CITIES = ["Dallas TX", "Atlanta GA", "Chicago IL", "Memphis TN", "Denver CO", "Phoenix AZ", "Columbus OH", "Laredo TX"]


def rng(seed):
    s = [seed & 0xFFFFFFFF]
    def nxt():
        s[0] = (s[0] * 1664525 + 1013904223) & 0xFFFFFFFF
        return s[0] / 2 ** 32
    return nxt


def js_round(x):
    return math.floor(x + 0.5)


def make_loads(n=40, seed=7):
    r = rng(seed)
    out = []
    for i in range(n):
        o = CITIES[math.floor(r() * 8)]
        d = CITIES[math.floor(r() * 8)]
        if d == o:
            d = CITIES[(CITIES.index(o) + 1) % 8]
        target = 1200 + js_round(r() * 2400 / 50) * 50
        out.append({"id": "L" + str(1000 + i), "origin": o, "dest": d, "equipment": "dry van" if r() < 0.7 else "reefer", "target": target, "max": target + 150 + js_round(r() * 6) * 50})
    return out


LOADS = make_loads()


_TRIM = re.compile("^" + _WS.pattern[:-1] + "+|" + _WS.pattern[:-1] + "+$")


def stt(text):
    """Simulated: trim and collapse whitespace."""
    return _WS.sub(" ", _TRIM.sub("", text))


_MC = re.compile(r"\bMC[\s#-]*(\d{5,7})\b", re.I | A)
_ID = re.compile(r"\bL\d{4}\b", re.I | A)
_RATE = re.compile(r"\$\s?(\d{3,5})|\b(\d{3,5})\s*(dollars|all in)\b", re.I | A)


def nlu(t):
    mc, lid, rate = _MC.search(t), _ID.search(t), _RATE.search(t)
    lane = [c for c in CITIES if c.split(" ")[0].lower() in t.lower()]
    return {"mc": mc.group(1) if mc else None, "loadId": lid.group(0).upper() if lid else None,
            "ask": int(rate.group(1) or rate.group(2)) if rate else None, "lane": lane}


class Lookup:
    def __init__(self, loads=None):
        self.loads = LOADS if loads is None else loads
        self.by_id = {l["id"]: l for l in self.loads}
        self.cache, self.hits, self.misses = {}, 0, 0

    def __call__(self, q, use_cache=True):
        if q["loadId"]:
            return self.by_id.get(q["loadId"])
        key = ">".join(q["lane"])
        if use_cache and key in self.cache:
            self.hits += 1
            return self.cache[key]
        self.misses += 1
        o = q["lane"][0] if q["lane"] else None
        d = q["lane"][1] if len(q["lane"]) > 1 else None
        m = next((l for l in self.loads if o and l["origin"].startswith(o.split(" ")[0]) and (not d or l["dest"].startswith(d.split(" ")[0]))), None)
        if use_cache:
            self.cache[key] = m
        return m

    def stats(self):
        return {"hits": self.hits, "misses": self.misses}


def make_lookup(loads=None):
    return Lookup(loads)


def negotiate(load, ask, rnd=1, mc="x"):
    """Guardrails: never offer above max, never reveal max/target, at most 3 counter rounds, require MC number before booking."""
    if not load:
        return {"action": "no_load", "offer": None}
    if not mc:
        return {"action": "need_mc", "offer": None}
    if ask is None:
        return {"action": "offer", "offer": load["target"]}
    if ask <= load["target"]:
        return {"action": "accept", "offer": ask}
    if ask <= load["max"] and rnd >= 2:
        return {"action": "accept", "offer": ask}
    if rnd >= 3:
        return {"action": "decline", "offer": None}
    return {"action": "counter", "offer": min(load["max"], js_round((load["target"] + min(ask, load["max"])) / 2 / 25) * 25)}


def say(load, n):
    a = n["action"]
    if a == "no_load": return "Sorry, I don't have a load on that lane right now. Can I take your MC number and call you back?"
    if a == "need_mc": return "Happy to help. What's your MC number?"
    if a == "offer": return f"I have {load['id']}, {load['origin']} to {load['dest']}, {load['equipment']}. I can do ${n['offer']} all in."
    if a == "accept": return f"Deal at ${n['offer']} for {load['id']}. I'll send the rate confirmation now."
    if a == "counter": return f"I can't do that, but I can do ${n['offer']} all in."
    return "I can't go that high on this one. Thanks for calling."


def summarize(call):
    return f"Call {call['id']}: carrier MC {call['mc'] or 'unknown'}, load {(call['load'] or {}).get('id') or 'none'}, outcome {call['outcome']}{' at $' + str(call['rate']) if call['rate'] else ''}, {call['turns']} turns."


def tts(text, size=12):
    """Simulated: text chunks stand in for audio frames."""
    for i in range(0, len(text), size):
        yield text[i:i + size]


def run_call(cid, utterances, lookup, clock=lambda: time.perf_counter() * 1000):
    st = {"stt": [], "nlu": [], "lookup": [], "negotiate": [], "tts_first_chunk": []}
    c = {"id": cid, "mc": None, "load": None, "outcome": "open", "rate": None, "turns": 0}
    out, rnd = [], 0
    for u in utterances:
        c["turns"] += 1
        t = clock(); text = stt(u); st["stt"].append(clock() - t)
        t = clock(); q = nlu(text); st["nlu"].append(clock() - t)
        c["mc"] = c["mc"] or q["mc"]
        t = clock()
        if q["loadId"] or q["lane"]:
            c["load"] = lookup(q) or c["load"]
        st["lookup"].append(clock() - t)
        t = clock()
        if q["ask"] is not None:
            rnd += 1
        n = negotiate(c["load"], q["ask"], rnd, c["mc"])
        st["negotiate"].append(clock() - t)
        if n["action"] == "accept":
            c["outcome"], c["rate"] = "booked", n["offer"]
        elif n["action"] == "decline":
            c["outcome"] = "declined"
        elif n["action"] == "no_load":
            c["outcome"] = "no_load"
        t = clock(); g = tts(say(c["load"], n)); out.append(next(g, None)); st["tts_first_chunk"].append(clock() - t)
        out.extend(g)
        if c["outcome"] in ("booked", "declined"):
            break
    t = clock(); summary = summarize(c); sum_ms = clock() - t
    return {"call": c, "summary": summary, "stages": {**st, "summary": [sum_ms]}, "out": out}


# ---- Inference cost & latency optimizer ----
# Prices: Cloudflare Workers AI published per-M-token prices (developers.cloudflare.com/workers-ai/platform/pricing, read 2026-10-27).
CONFIGS = {"template": {"in": 0, "out": 0}, "llama-3.2-1b": {"in": 0.027, "out": 0.201}, "llama-3.1-8b-fast": {"in": 0.045, "out": 0.384}, "llama-3.1-70b-fast": {"in": 0.293, "out": 2.253}}


def toks(s):
    return math.ceil(len(s) / 4)  # token estimate (chars/4), not a tokenizer count


def cost(cfg, tin, tout):
    p = CONFIGS[cfg]
    return (tin * p["in"] + tout * p["out"]) / 1e6


def route(turn_type, profiles, target):
    """profiles[cfg][turnType] = {quality, p95ms}; only measured profiles are used. Unmeasured configs are skipped."""
    ok = []
    for c in CONFIGS:
        p = (profiles.get(c) or {}).get(turn_type)
        if p and p["quality"] >= target["quality"] and p["p95ms"] <= target["p95ms"]:
            ok.append(c)
    ok.sort(key=lambda c: cost(c, 1000, 100))
    return ok[0] if ok else "llama-3.1-70b-fast"


class Batcher:
    def __init__(self, mx=8):
        self.max, self.q, self.requests = mx, [], 0
    def add(self, x):
        self.q.append(x)
        if len(self.q) >= self.max:
            self.flush()
    def flush(self):
        if self.q:
            self.requests += 1
            self.q = []
