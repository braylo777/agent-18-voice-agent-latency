//! Freight carrier-call agent pipeline: STT(sim) -> NLU -> load lookup -> negotiation (guardrails) -> summary -> TTS(sim).
//! STT and TTS are SIMULATED with text: input arrives as text, output streams as text chunks.
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const CITIES: [&str; 8] = [
    "Dallas TX",
    "Atlanta GA",
    "Chicago IL",
    "Memphis TN",
    "Denver CO",
    "Phoenix AZ",
    "Columbus OH",
    "Laredo TX",
];
const JS_WS: &str = r"[\t\n\x0B\x0C\r \x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}]+";

pub fn rng(seed: u32) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        s as f64 / 4294967296.0
    }
}
fn js_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Load {
    pub id: String,
    pub origin: String,
    pub dest: String,
    pub equipment: String,
    pub target: i64,
    pub max: i64,
}

pub fn make_loads(n: usize, seed: u32) -> Vec<Load> {
    let mut r = rng(seed);
    (0..n)
        .map(|i| {
            let o = CITIES[(r() * 8.0).floor() as usize];
            let mut d = CITIES[(r() * 8.0).floor() as usize];
            if d == o {
                d = CITIES[(CITIES.iter().position(|c| *c == o).unwrap() + 1) % 8];
            }
            let target = 1200 + js_round(r() * 2400.0 / 50.0) * 50;
            let equipment = if r() < 0.7 { "dry van" } else { "reefer" };
            let max = target + 150 + js_round(r() * 6.0) * 50;
            Load {
                id: format!("L{}", 1000 + i),
                origin: o.into(),
                dest: d.into(),
                equipment: equipment.into(),
                target,
                max,
            }
        })
        .collect()
}

pub fn loads() -> &'static Vec<Load> {
    static L: OnceLock<Vec<Load>> = OnceLock::new();
    L.get_or_init(|| make_loads(40, 7))
}

pub fn stt(text: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let ws = R.get_or_init(|| Regex::new(JS_WS).unwrap());
    let is_ws = |c: char| ws.is_match(c.encode_utf8(&mut [0; 4]));
    ws.replace_all(text.trim_matches(is_ws), " ").into_owned()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Query {
    pub mc: Option<String>,
    #[serde(rename = "loadId")]
    pub load_id: Option<String>,
    pub ask: Option<i64>,
    pub lane: Vec<String>,
}

pub fn nlu(t: &str) -> Query {
    static R: OnceLock<[Regex; 3]> = OnceLock::new();
    let r = R.get_or_init(|| {
        let b = |p: &str| Regex::new(&format!("(?i){}", p.replace(r"\b", r"(?-u:\b)"))).unwrap();
        [
            b(r"\bMC[\s#-]*([0-9]{5,7})\b"),
            b(r"\bL[0-9]{4}\b"),
            b(r"\$\s?([0-9]{3,5})|\b([0-9]{3,5})\s*(dollars|all in)\b"),
        ]
    });
    let low = t.to_lowercase();
    Query {
        mc: r[0].captures(t).map(|c| c[1].to_string()),
        load_id: r[1].find(t).map(|m| m.as_str().to_uppercase()),
        ask: r[2]
            .captures(t)
            .map(|c| c.get(1).or(c.get(2)).unwrap().as_str().parse().unwrap()),
        lane: CITIES
            .iter()
            .filter(|c| low.contains(&c.split(' ').next().unwrap().to_lowercase()))
            .map(|c| c.to_string())
            .collect(),
    }
}

pub struct Lookup {
    loads: Vec<Load>,
    by_id: HashMap<String, usize>,
    cache: HashMap<String, Option<usize>>,
    pub hits: u64,
    pub misses: u64,
}
impl Lookup {
    pub fn new(loads: Vec<Load>) -> Self {
        let by_id = loads
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect::<HashMap<_, _>>();
        // JS Map keeps the LAST load for a duplicate id; mirror that.
        let mut last: HashMap<String, usize> = HashMap::new();
        for (i, l) in loads.iter().enumerate() {
            last.insert(l.id.clone(), i);
        }
        let _ = by_id;
        Lookup {
            loads,
            by_id: last,
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }
    pub fn standard() -> Self {
        Self::new(loads().clone())
    }
    pub fn get(&mut self, q: &Query, use_cache: bool) -> Option<&Load> {
        if let Some(id) = &q.load_id {
            return self.by_id.get(id).map(|i| &self.loads[*i]);
        }
        let key = q.lane.join(">");
        if use_cache {
            if let Some(v) = self.cache.get(&key) {
                self.hits += 1;
                return v.map(|i| &self.loads[i]);
            }
        }
        self.misses += 1;
        let (o, d) = (q.lane.first(), q.lane.get(1));
        let m = o.and_then(|o| {
            let of = o.split(' ').next().unwrap();
            self.loads.iter().position(|l| {
                l.origin.starts_with(of)
                    && d.map_or(true, |d| l.dest.starts_with(d.split(' ').next().unwrap()))
            })
        });
        if use_cache {
            self.cache.insert(key, m);
        }
        m.map(|i| &self.loads[i])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Neg {
    pub action: &'static str,
    pub offer: Option<i64>,
}

/// Guardrails: never offer above max, never reveal max/target, at most 3 counter rounds, require MC number before booking.
pub fn negotiate(load: Option<&Load>, ask: Option<i64>, round: u32, mc: Option<&str>) -> Neg {
    let Some(load) = load else {
        return Neg {
            action: "no_load",
            offer: None,
        };
    };
    if mc.map_or(true, |m| m.is_empty()) {
        return Neg {
            action: "need_mc",
            offer: None,
        };
    }
    let Some(ask) = ask else {
        return Neg {
            action: "offer",
            offer: Some(load.target),
        };
    };
    if ask <= load.target {
        return Neg {
            action: "accept",
            offer: Some(ask),
        };
    }
    if ask <= load.max && round >= 2 {
        return Neg {
            action: "accept",
            offer: Some(ask),
        };
    }
    if round >= 3 {
        return Neg {
            action: "decline",
            offer: None,
        };
    }
    let counter = load
        .max
        .min(js_round((load.target + ask.min(load.max)) as f64 / 2.0 / 25.0) * 25);
    Neg {
        action: "counter",
        offer: Some(counter),
    }
}

pub fn say(load: Option<&Load>, n: &Neg) -> String {
    let o = n.offer.unwrap_or(0);
    match n.action {
        "no_load" => "Sorry, I don't have a load on that lane right now. Can I take your MC number and call you back?".into(),
        "need_mc" => "Happy to help. What's your MC number?".into(),
        "offer" => { let l = load.unwrap(); format!("I have {}, {} to {}, {}. I can do ${} all in.", l.id, l.origin, l.dest, l.equipment, o) }
        "accept" => format!("Deal at ${} for {}. I'll send the rate confirmation now.", o, load.unwrap().id),
        "counter" => format!("I can't do that, but I can do ${} all in.", o),
        _ => "I can't go that high on this one. Thanks for calling.".into(),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Call {
    pub id: String,
    pub mc: Option<String>,
    pub load: Option<Load>,
    pub outcome: String,
    pub rate: Option<i64>,
    pub turns: u32,
}

pub fn summarize(c: &Call) -> String {
    format!(
        "Call {}: carrier MC {}, load {}, outcome {}{}, {} turns.",
        c.id,
        c.mc.as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or("unknown"),
        c.load.as_ref().map_or("none", |l| l.id.as_str()),
        c.outcome,
        c.rate
            .filter(|r| *r != 0)
            .map_or(String::new(), |r| format!(" at ${r}")),
        c.turns
    )
}

/// Simulated: text chunks stand in for audio frames (12 UTF-16 units per chunk).
pub fn tts(text: &str, size: usize) -> Vec<String> {
    let u: Vec<u16> = text.encode_utf16().collect();
    u.chunks(size).map(String::from_utf16_lossy).collect()
}

#[derive(Debug, Default)]
pub struct Stages {
    pub stt: Vec<f64>,
    pub nlu: Vec<f64>,
    pub lookup: Vec<f64>,
    pub negotiate: Vec<f64>,
    pub tts_first_chunk: Vec<f64>,
    pub summary: Vec<f64>,
}

pub struct CallRun {
    pub call: Call,
    pub summary: String,
    pub stages: Stages,
    pub out: Vec<String>,
}

/// `clock` returns milliseconds.
pub fn run_call(
    id: &str,
    utterances: &[String],
    lookup: &mut Lookup,
    clock: &dyn Fn() -> f64,
) -> CallRun {
    let mut st = Stages::default();
    let mut c = Call {
        id: id.into(),
        mc: None,
        load: None,
        outcome: "open".into(),
        rate: None,
        turns: 0,
    };
    let (mut out, mut round) = (vec![], 0u32);
    for u in utterances {
        c.turns += 1;
        let mut t = clock();
        let text = stt(u);
        st.stt.push(clock() - t);
        t = clock();
        let q = nlu(&text);
        st.nlu.push(clock() - t);
        if c.mc.as_deref().map_or(true, |m| m.is_empty()) {
            c.mc = q.mc.clone();
        }
        t = clock();
        if q.load_id.is_some() || !q.lane.is_empty() {
            if let Some(l) = lookup.get(&q, true) {
                c.load = Some(l.clone());
            }
        }
        st.lookup.push(clock() - t);
        t = clock();
        if q.ask.is_some() {
            round += 1;
        }
        let n = negotiate(c.load.as_ref(), q.ask, round, c.mc.as_deref());
        st.negotiate.push(clock() - t);
        match n.action {
            "accept" => {
                c.outcome = "booked".into();
                c.rate = n.offer;
            }
            "decline" => c.outcome = "declined".into(),
            "no_load" => c.outcome = "no_load".into(),
            _ => {}
        }
        t = clock();
        let chunks = tts(&say(c.load.as_ref(), &n), 12);
        st.tts_first_chunk.push(clock() - t);
        out.extend(chunks);
        if c.outcome == "booked" || c.outcome == "declined" {
            break;
        }
    }
    let t = clock();
    let summary = summarize(&c);
    st.summary.push(clock() - t);
    CallRun {
        call: c,
        summary,
        stages: st,
        out,
    }
}

// ---- Inference cost & latency optimizer ----
// Prices: Cloudflare Workers AI published per-M-token prices (developers.cloudflare.com/workers-ai/platform/pricing, read 2026-10-27).
pub const CONFIGS: [(&str, f64, f64); 4] = [
    ("template", 0.0, 0.0),
    ("llama-3.2-1b", 0.027, 0.201),
    ("llama-3.1-8b-fast", 0.045, 0.384),
    ("llama-3.1-70b-fast", 0.293, 2.253),
];

/// Token estimate (chars/4), not a tokenizer count.
pub fn toks(s: &str) -> i64 {
    (s.encode_utf16().count() as f64 / 4.0).ceil() as i64
}
pub fn cost(cfg: &str, tin: f64, tout: f64) -> f64 {
    let p = CONFIGS.iter().find(|c| c.0 == cfg).unwrap();
    (tin * p.1 + tout * p.2) / 1e6
}

/// profiles: (config, turn type, quality, p95 ms); only measured profiles are used. Unmeasured configs are skipped.
pub fn route(
    turn_type: &str,
    profiles: &[(&str, &str, f64, f64)],
    quality: f64,
    p95ms: f64,
) -> &'static str {
    let mut ok: Vec<&'static str> = CONFIGS
        .iter()
        .map(|c| c.0)
        .filter(|c| {
            profiles
                .iter()
                .any(|p| p.0 == *c && p.1 == turn_type && p.2 >= quality && p.3 <= p95ms)
        })
        .collect();
    ok.sort_by(|a, b| {
        cost(a, 1000.0, 100.0)
            .partial_cmp(&cost(b, 1000.0, 100.0))
            .unwrap()
    });
    ok.first().copied().unwrap_or("llama-3.1-70b-fast")
}

pub struct Batcher {
    max: usize,
    q: usize,
    pub requests: u32,
}
impl Batcher {
    pub fn new(max: usize) -> Self {
        Batcher {
            max,
            q: 0,
            requests: 0,
        }
    }
    pub fn add(&mut self) {
        self.q += 1;
        if self.q >= self.max {
            self.flush();
        }
    }
    pub fn flush(&mut self) {
        if self.q > 0 {
            self.requests += 1;
            self.q = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn l0() -> Load {
        loads()[0].clone()
    }
    fn clk() -> impl Fn() -> f64 {
        let c = std::cell::Cell::new(0.0);
        move || {
            c.set(c.get() + 0.001);
            c.get()
        }
    }
    #[test]
    fn stt_nlu_lookup() {
        assert_eq!(stt("  a   b "), "a b");
        let q = nlu("on L1003 MC 123456 need $2500 all in");
        assert_eq!((q.mc.as_deref(), q.ask), (Some("123456"), Some(2500)));
        assert_eq!(nlu("L1003").load_id.as_deref(), Some("L1003"));
        assert_eq!(nlu("need $2500").ask, Some(2500));
        let mut lane = nlu("anything Dallas to Memphis").lane;
        lane.sort();
        assert_eq!(lane, ["Dallas TX", "Memphis TN"]);
        let mut lk = Lookup::standard();
        assert_eq!(
            lk.get(
                &Query {
                    mc: None,
                    load_id: Some("L1000".into()),
                    ask: None,
                    lane: vec![]
                },
                true
            )
            .unwrap()
            .id,
            "L1000"
        );
        let q = Query {
            mc: None,
            load_id: None,
            ask: None,
            lane: vec![l0().origin],
        };
        lk.get(&q, true);
        lk.get(&q, true);
        assert_eq!(lk.hits, 1);
    }
    #[test]
    fn guardrails_and_calls() {
        let l = l0();
        let mut a = l.target;
        while a < l.max + 2000 {
            for r in 1..=4 {
                if let Some(o) = negotiate(Some(&l), Some(a), r, Some("x")).offer {
                    assert!(o <= l.max);
                }
            }
            a += 37;
        }
        assert_eq!(negotiate(Some(&l), Some(100), 1, None).action, "need_mc");
        assert_eq!(
            negotiate(Some(&l), Some(l.max + 500), 3, Some("x")).action,
            "decline"
        );
        assert_eq!(
            negotiate(Some(&l), Some(l.target - 50), 1, Some("x")).action,
            "accept"
        );
        let mut lk = Lookup::standard();
        let r = run_call(
            "t",
            &[format!("on {} MC 123456", l.id), format!("${}", l.target)],
            &mut lk,
            &clk(),
        );
        assert_eq!(r.call.outcome, "booked");
        let r = run_call(
            "t",
            &["Boise to Reno MC 123456".to_string()],
            &mut Lookup::standard(),
            &clk(),
        );
        assert_eq!(r.call.outcome, "no_load");
        assert_eq!(
            tts("hello world, freight", 12).concat(),
            "hello world, freight"
        );
    }
    #[test]
    fn router_cost_batcher() {
        let p = [
            ("llama-3.2-1b", "s", 0.9, 200.0),
            ("llama-3.1-8b-fast", "s", 0.97, 300.0),
        ];
        assert_eq!(route("s", &p, 0.95, 500.0), "llama-3.1-8b-fast");
        assert_eq!(route("s", &[], 0.95, 1.0), "llama-3.1-70b-fast");
        let p = [
            ("llama-3.2-1b", "s", 1.0, 900.0),
            ("llama-3.1-8b-fast", "s", 1.0, 300.0),
        ];
        assert_eq!(route("s", &p, 0.9, 500.0), "llama-3.1-8b-fast");
        assert_eq!(cost("llama-3.1-70b-fast", 1e6, 0.0), 0.293);
        let mut b = Batcher::new(8);
        for _ in 0..20 {
            b.add();
        }
        b.flush();
        assert_eq!(b.requests, 3);
    }
}
