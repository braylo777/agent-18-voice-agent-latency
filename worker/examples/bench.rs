// Per-stage latency of the Rust pipeline (50 reps x 60 seeded calls). Run: cargo run --release --example bench
use agent_18_voice_agent_latency::core::*;
use std::time::Instant;
fn pct(v: &mut Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((p / 100.0 * v.len() as f64).floor() as usize).min(v.len() - 1)]
}
fn main() {
    let ld = loads();
    let mut r = rng(11);
    let calls: Vec<Vec<String>> = (0..60).map(|_| {
        let l = &ld[(r() * 12.0).floor() as usize];
        let mc = format!("MC {}", 100000 + (r() * 899999.0).floor() as i64);
        let k = r();
        if k < 0.15 { return vec![format!("Hi, calling about a load out of Boise to Reno, {mc}")]; }
        let a = if r() < 0.5 { format!("Hey, calling on {}, {mc}", l.id) } else { format!("Got anything {} to {}? {mc}", l.origin.split(' ').next().unwrap(), l.dest.split(' ').next().unwrap()) };
        let ask = l.target + 50 * (r() * 12.0).floor() as i64 - 100;
        vec![a, "What's it paying?".into(), format!("I need ${ask} all in"), format!("Can you do ${}", (ask - 50).max(l.target)), format!("Best I can do is ${}", ask - 25)]
    }).collect();
    let t0 = Instant::now();
    let clock = move || t0.elapsed().as_secs_f64() * 1000.0;
    let mut agg: [Vec<f64>; 6] = Default::default();
    for _ in 0..50 {
        let mut lk = Lookup::standard();
        for (i, u) in calls.iter().enumerate() {
            let r = run_call(&format!("C{i}"), u, &mut lk, &clock);
            for (k, v) in [r.stages.stt, r.stages.nlu, r.stages.lookup, r.stages.negotiate, r.stages.tts_first_chunk, r.stages.summary].into_iter().enumerate() { agg[k].extend(v); }
        }
    }
    for (name, v) in ["stt", "nlu", "lookup", "negotiate", "tts_first_chunk", "summary"].iter().zip(agg.iter_mut()) {
        println!("{name}: p50 {:.4} ms  p95 {:.4} ms  n={}", pct(v, 50.0), pct(v, 95.0), v.len());
    }
}
