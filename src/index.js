import { runCall, makeLookup, LOADS } from "./core.js";
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
