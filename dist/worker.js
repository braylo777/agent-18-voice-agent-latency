// Freight carrier-call agent pipeline: STT(sim) -> NLU -> load lookup -> negotiation (guardrails) -> summary -> TTS(sim).
// STT and TTS are SIMULATED with text: input arrives as text, output streams as text chunks.
function rng(seed) { let s = seed >>> 0; return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32); }
const CITIES = ["Dallas TX", "Atlanta GA", "Chicago IL", "Memphis TN", "Denver CO", "Phoenix AZ", "Columbus OH", "Laredo TX"];
function makeLoads(n = 40, seed = 7) { const r = rng(seed); return Array.from({ length: n }, (_, i) => { const o = CITIES[Math.floor(r() * 8)]; let d = CITIES[Math.floor(r() * 8)]; if (d === o) d = CITIES[(CITIES.indexOf(o) + 1) % 8]; const target = 1200 + Math.round(r() * 2400 / 50) * 50; return { id: "L" + (1000 + i), origin: o, dest: d, equipment: r() < 0.7 ? "dry van" : "reefer", target, max: target + 150 + Math.round(r() * 6) * 50 }; }); }
const LOADS = makeLoads();
function stt(text) { return text.trim().replace(/\s+/g, " "); } // simulated
function nlu(t) { const mc = t.match(/\bMC[\s#-]*(\d{5,7})\b/i); const id = t.match(/\bL\d{4}\b/i); const rate = t.match(/\$\s?(\d{3,5})|\b(\d{3,5})\s*(dollars|all in)\b/i);
  const lane = CITIES.filter((c) => t.toLowerCase().includes(c.split(" ")[0].toLowerCase())); return { mc: mc?.[1] || null, loadId: id?.[0].toUpperCase() || null, ask: rate ? +(rate[1] || rate[2]) : null, lane }; }
function makeLookup(loads = LOADS) { const byId = new Map(loads.map((l) => [l.id, l])); const cache = new Map(); let hits = 0, misses = 0;
  const fn = (q, useCache = true) => { if (q.loadId) return byId.get(q.loadId) || null; const key = q.lane.join(">"); if (useCache && cache.has(key)) { hits++; return cache.get(key); } misses++;
    const [o, d] = q.lane; const m = loads.find((l) => o && l.origin.startsWith(o.split(" ")[0]) && (!d || l.dest.startsWith(d.split(" ")[0]))) || null; if (useCache) cache.set(key, m); return m; };
  fn.stats = () => ({ hits, misses }); return fn; }
// Guardrails: never offer above max, never reveal max/target, at most 3 counter rounds, require MC number before booking.
function negotiate(load, ask, round = 1, mc = "x") {
  if (!load) return { action: "no_load", offer: null };
  if (!mc) return { action: "need_mc", offer: null };
  if (ask == null) return { action: "offer", offer: load.target };
  if (ask <= load.target) return { action: "accept", offer: ask };
  if (ask <= load.max && round >= 2) return { action: "accept", offer: ask };
  if (round >= 3) return { action: "decline", offer: null };
  const counter = Math.min(load.max, Math.round((load.target + Math.min(ask, load.max)) / 2 / 25) * 25); return { action: "counter", offer: counter };
}
function say(load, n) { switch (n.action) { case "no_load": return "Sorry, I don't have a load on that lane right now. Can I take your MC number and call you back?";
  case "need_mc": return "Happy to help. What's your MC number?"; case "offer": return `I have ${load.id}, ${load.origin} to ${load.dest}, ${load.equipment}. I can do $${n.offer} all in.`;
  case "accept": return `Deal at $${n.offer} for ${load.id}. I'll send the rate confirmation now.`; case "counter": return `I can't do that, but I can do $${n.offer} all in.`; default: return "I can't go that high on this one. Thanks for calling."; } }
function summarize(call) { return `Call ${call.id}: carrier MC ${call.mc || "unknown"}, load ${call.load?.id || "none"}, outcome ${call.outcome}${call.rate ? " at $" + call.rate : ""}, ${call.turns} turns.`; }
function* tts(text, size = 12) { for (let i = 0; i < text.length; i += size) yield text.slice(i, i + size); } // simulated: text chunks stand in for audio frames
function runCall(id, utterances, lookup, clock = () => performance.now()) {
  const st = { stt: [], nlu: [], lookup: [], negotiate: [], tts_first_chunk: [] }; const c = { id, mc: null, load: null, outcome: "open", rate: null, turns: 0 }; const out = []; let round = 0;
  for (const u of utterances) { c.turns++; let t = clock(); const text = stt(u); st.stt.push(clock() - t); t = clock(); const q = nlu(text); st.nlu.push(clock() - t); c.mc ||= q.mc;
    t = clock(); if (q.loadId || q.lane.length) c.load = lookup(q) || c.load; st.lookup.push(clock() - t);
    t = clock(); if (q.ask != null) round++; const n = negotiate(c.load, q.ask, round, c.mc); st.negotiate.push(clock() - t);
    if (n.action === "accept") { c.outcome = "booked"; c.rate = n.offer; } else if (n.action === "decline") c.outcome = "declined"; else if (n.action === "no_load") c.outcome = "no_load";
    t = clock(); const g = tts(say(c.load, n)); out.push(g.next().value); st.tts_first_chunk.push(clock() - t); for (const ch of g) out.push(ch); if (c.outcome === "booked" || c.outcome === "declined") break; }
  let t = clock(); const summary = summarize(c); const sumMs = clock() - t; return { call: c, summary, stages: { ...st, summary: [sumMs] }, out };
}
// ---- Inference cost & latency optimizer ----
// Prices: Cloudflare Workers AI published per-M-token prices (developers.cloudflare.com/workers-ai/platform/pricing, read 2026-10-27).
const CONFIGS = { template: { in: 0, out: 0 }, "llama-3.2-1b": { in: 0.027, out: 0.201 }, "llama-3.1-8b-fast": { in: 0.045, out: 0.384 }, "llama-3.1-70b-fast": { in: 0.293, out: 2.253 } };
const toks = (s) => Math.ceil(s.length / 4); // token estimate (chars/4), not a tokenizer count
function cost(cfg, tin, tout) { const p = CONFIGS[cfg]; return (tin * p.in + tout * p.out) / 1e6; }
// profiles[cfg][turnType] = {quality, p95ms}; only measured profiles are used. Unmeasured configs are skipped.
function route(turnType, profiles, target) { const ok = Object.keys(CONFIGS).filter((c) => { const p = profiles[c]?.[turnType]; return p && p.quality >= target.quality && p.p95ms <= target.p95ms; });
  ok.sort((a, b) => cost(a, 1000, 100) - cost(b, 1000, 100)); return ok[0] || "llama-3.1-70b-fast"; }
class Batcher { constructor(max = 8) { this.max = max; this.q = []; this.requests = 0; } add(x) { this.q.push(x); if (this.q.length >= this.max) this.flush(); } flush() { if (this.q.length) { this.requests++; this.q = []; } } }

const lookup = makeLookup();
const page = `<!doctype html><meta charset=utf-8><meta name=viewport content="width=device-width"><title>agent-18 carrier-call agent</title><body style="font:15px system-ui;max-width:760px;margin:2em auto;padding:0 1em"><h1>agent-18 · Freight carrier-call agent (streaming)</h1><p>Load lookup → guardrailed rate negotiation → call summary, streamed as SSE. STT/TTS are <b>simulated with text</b>. <a href="https://github.com/braylo777/agent-18-voice-agent-latency">GitHub</a></p><textarea id=t rows=5 style="width:100%">Hey, calling on L1000, MC 123456
What's it paying?
I need $3000 all in
Can you do $2900</textarea><br><button onclick="go()">Run call</button> <span id=ms></span><pre id=o style="white-space:pre-wrap;background:#f4f4f4;padding:1em"></pre><script>async function go(){o.textContent='';const t0=performance.now();let first=0;const r=await fetch('/api/call',{method:'POST',body:JSON.stringify({utterances:t.value.split('\\n')})});const rd=r.body.getReader(),d=new TextDecoder();for(;;){const{value,done}=await rd.read();if(done)break;if(!first){first=performance.now()-t0}o.textContent+=d.decode(value)}ms.textContent='first token '+first.toFixed(0)+' ms, total '+(performance.now()-t0).toFixed(0)+' ms'}</script>`;
export default { async fetch(req) { const u = new URL(req.url);
  if (u.pathname === "/api/call" && req.method === "POST") { let b; try { b = await req.json(); } catch { return new Response("bad json", { status: 400 }); }
    const utt = (Array.isArray(b.utterances) ? b.utterances : []).slice(0, 10).map((x) => String(x).slice(0, 500)); if (!utt.length) return new Response("no utterances", { status: 400 });
    const r = runCall("web", utt, lookup); const enc = new TextEncoder();
    const stream = new ReadableStream({ start(c) { for (const ch of r.out) c.enqueue(enc.encode("data: " + JSON.stringify({ tts: ch }) + "\n\n")); c.enqueue(enc.encode("data: " + JSON.stringify({ summary: r.summary, outcome: r.call.outcome }) + "\n\n")); c.close(); } });
    return new Response(stream, { headers: { "content-type": "text/event-stream", "cache-control": "no-store", "access-control-allow-origin": "*" } }); }
  if (u.pathname === "/api/loads") return Response.json(LOADS.map(({ max, ...l }) => l)); // never expose ceiling
  if (u.pathname === "/health") return Response.json({ ok: true });
  return new Response(page, { headers: { "content-type": "text/html; charset=utf-8" } }); } };
