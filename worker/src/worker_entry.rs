//! Cloudflare Worker entry (wasm32 only). Same routes, SSE stream and CORS as the old src/index.js.
use crate::core::{loads, run_call, Lookup};
use crate::page::PAGE;
use serde_json::{json, Value};
use std::cell::RefCell;
use worker::*;

thread_local! { static LOOKUP: RefCell<Lookup> = RefCell::new(Lookup::standard()); }

#[event(fetch)]
async fn fetch(mut req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let path = req.url()?.path().to_string();
    if path == "/api/call" && req.method() == Method::Post {
        let Ok(b) = req.json::<Value>().await else { return Response::error("bad json", 400) };
        let utt: Vec<String> = b.get("utterances").and_then(|u| u.as_array()).map(|a| a.iter().take(10).map(|x| match x { Value::String(s) => s.clone(), o => o.to_string() }.encode_utf16().take(500).collect::<Vec<u16>>()).map(|u| String::from_utf16_lossy(&u)).collect()).unwrap_or_default();
        if utt.is_empty() {
            return Response::error("no utterances", 400);
        }
        let clock = || js_sys::Date::now();
        let r = LOOKUP.with(|l| run_call("web", &utt, &mut l.borrow_mut(), &clock));
        let mut body = String::new();
        for ch in &r.out {
            body.push_str(&format!("data: {}\n\n", json!({"tts": ch})));
        }
        body.push_str(&format!("data: {}\n\n", json!({"summary": r.summary, "outcome": r.call.outcome})));
        let h = Headers::new();
        h.set("content-type", "text/event-stream")?;
        h.set("cache-control", "no-store")?;
        h.set("access-control-allow-origin", "*")?;
        return Ok(Response::ok(body)?.with_headers(h));
    }
    if path == "/api/loads" {
        let v: Vec<Value> = loads().iter().map(|l| json!({"id": l.id, "origin": l.origin, "dest": l.dest, "equipment": l.equipment, "target": l.target})).collect(); // never expose ceiling
        return Response::from_json(&v);
    }
    if path == "/health" {
        return Response::from_json(&json!({"ok": true}));
    }
    let h = Headers::new();
    h.set("content-type", "text/html; charset=utf-8")?;
    Ok(Response::ok(PAGE)?.with_headers(h))
}
