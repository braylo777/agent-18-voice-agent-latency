// 60 synthetic carrier calls (seeded). Synthetic data, not real carrier transcripts.
import { LOADS, rng } from "../src/core.js";
export function makeCalls(n = 60, seed = 11) { const r = rng(seed); const calls = [];
  for (let i = 0; i < n; i++) { const l = LOADS[Math.floor(r() * 12)]; const mc = "MC " + (100000 + Math.floor(r() * 899999)); const k = r(); const u = [];
    if (k < 0.15) u.push(`Hi, calling about a load out of Boise to Reno, ${mc}`); else { u.push(r() < 0.5 ? `Hey, calling on ${l.id}, ${mc}` : `Got anything ${l.origin.split(" ")[0]} to ${l.dest.split(" ")[0]}? ${mc}`); u.push("What's it paying?");
      const ask = l.target + 50 * Math.floor(r() * 12) - 100; u.push(`I need $${ask} all in`); u.push(`Can you do $${Math.max(ask - 50, l.target)}`); u.push(`Best I can do is $${ask - 25}`); }
    calls.push({ id: "C" + i, u }); } return calls; }
