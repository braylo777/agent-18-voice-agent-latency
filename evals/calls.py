"""60 synthetic carrier calls (seeded). Synthetic data, not real carrier transcripts."""

import math
from agent18.core import LOADS, rng


def make_calls(n=60, seed=11):
    r = rng(seed)
    calls = []
    for i in range(n):
        l = LOADS[math.floor(r() * 12)]
        mc = "MC " + str(100000 + math.floor(r() * 899999))
        k = r()
        u = []
        if k < 0.15:
            u.append(f"Hi, calling about a load out of Boise to Reno, {mc}")
        else:
            u.append(
                f"Hey, calling on {l['id']}, {mc}"
                if r() < 0.5
                else f"Got anything {l['origin'].split(' ')[0]} to {l['dest'].split(' ')[0]}? {mc}"
            )
            u.append("What's it paying?")
            ask = l["target"] + 50 * math.floor(r() * 12) - 100
            u.append(f"I need ${ask} all in")
            u.append(f"Can you do ${max(ask - 50, l['target'])}")
            u.append(f"Best I can do is ${ask - 25}")
        calls.append({"id": f"C{i}", "u": u})
    return calls
