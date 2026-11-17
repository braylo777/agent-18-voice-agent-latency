// Parity helper: loads, 60 seeded calls, odd NLU inputs; prints JSON like the legacy JS harness.
use agent_18_voice_agent_latency::core::*;
use serde_json::{json, Value};
fn calls() -> Vec<(String, Vec<String>)> {
    let ld = loads();
    let mut r = rng(11);
    (0..60)
        .map(|i| {
            let l = &ld[(r() * 12.0).floor() as usize];
            let mc = format!("MC {}", 100000 + (r() * 899999.0).floor() as i64);
            let k = r();
            let mut u = vec![];
            if k < 0.15 {
                u.push(format!("Hi, calling about a load out of Boise to Reno, {mc}"));
            } else {
                u.push(if r() < 0.5 { format!("Hey, calling on {}, {mc}", l.id) } else { format!("Got anything {} to {}? {mc}", l.origin.split(' ').next().unwrap(), l.dest.split(' ').next().unwrap()) });
                u.push("What's it paying?".into());
                let ask = l.target + 50 * (r() * 12.0).floor() as i64 - 100;
                u.push(format!("I need ${ask} all in"));
                u.push(format!("Can you do ${}", (ask - 50).max(l.target)));
                u.push(format!("Best I can do is ${}", ask - 25));
            }
            (format!("C{i}"), u)
        })
        .collect()
}
fn main() {
    let cs = calls();
    let mut lk = Lookup::standard();
    let clock = || 0.0;
    let runs: Vec<Value> = cs.iter().map(|(id, u)| { let r = run_call(id, u, &mut lk, &clock); json!({"call": r.call, "summary": r.summary, "out": r.out}) }).collect();
    let odd = ["  hey  there ", "on l1003 mc#123456 $2,500", "Dallas to Memphis 3000 dollars", "MC 12345 need 450 all in", "", "L99999 L1001x", "\t\u{a0}x\u{2003}y"];
    println!("{}", serde_json::to_string(&json!({"loads": loads(), "calls": cs.iter().map(|(id, u)| json!({"id": id, "u": u})).collect::<Vec<_>>(), "runs": runs,
        "odd": odd.iter().map(|t| json!({"s": stt(t), "n": nlu(&stt(t))})).collect::<Vec<_>>(), "loads2": make_loads(10, 99), "stats": json!({"hits": lk.hits, "misses": lk.misses})})).unwrap());
}
