import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / "evals"))
from agent18.core import (
    nlu,
    negotiate,
    make_lookup,
    LOADS,
    run_call,
    route,
    cost,
    Batcher,
    tts,
    stt,
)  # noqa: E402
from bench import bench  # noqa: E402

L = LOADS[0]


def test_stt_nlu():
    assert stt("  a   b ") == "a b"
    assert [
        nlu("on L1003 MC 123456 need $2500 all in")["mc"],
        nlu("L1003")["loadId"],
        nlu("need $2500")["ask"],
    ] == ["123456", "L1003", 2500]
    assert sorted(nlu("anything Dallas to Memphis")["lane"]) == [
        "Dallas TX",
        "Memphis TN",
    ]


def test_lookup():
    assert make_lookup()({"loadId": "L1000", "lane": []})["id"] == "L1000"
    lk = make_lookup()
    q = {"loadId": None, "lane": [L["origin"]]}
    lk(q)
    lk(q)
    assert lk.stats()["hits"] == 1


def test_guardrails():
    for a in range(L["target"], L["max"] + 2000, 37):
        for r in range(1, 5):
            n = negotiate(L, a, r)
            if n["offer"]:
                assert n["offer"] <= L["max"]
    assert negotiate(L, 100, 1, None)["action"] == "need_mc"
    assert negotiate(L, L["max"] + 500, 3)["action"] == "decline"
    assert negotiate(L, L["target"] - 50)["action"] == "accept"


def test_calls_and_tts():
    assert (
        run_call("t", [f"on {L['id']} MC 123456", f"${L['target']}"], make_lookup())[
            "call"
        ]["outcome"]
        == "booked"
    )
    assert (
        run_call("t", ["Boise to Reno MC 123456"], make_lookup())["call"]["outcome"]
        == "no_load"
    )
    assert "".join(tts("hello world, freight")) == "hello world, freight"


def test_router_cost_batcher():
    p = {
        "llama-3.2-1b": {"s": {"quality": 0.9, "p95ms": 200}},
        "llama-3.1-8b-fast": {"s": {"quality": 0.97, "p95ms": 300}},
    }
    assert route("s", p, {"quality": 0.95, "p95ms": 500}) == "llama-3.1-8b-fast"
    assert route("s", {}, {"quality": 0.95, "p95ms": 1}) == "llama-3.1-70b-fast"
    p = {
        "llama-3.2-1b": {"s": {"quality": 1, "p95ms": 900}},
        "llama-3.1-8b-fast": {"s": {"quality": 1, "p95ms": 300}},
    }
    assert route("s", p, {"quality": 0.9, "p95ms": 500}) == "llama-3.1-8b-fast"
    assert cost("llama-3.1-70b-fast", 1e6, 0) == 0.293
    b = Batcher(8)
    for i in range(20):
        b.add(i)
    b.flush()
    assert b.requests == 3


def test_bench_zero_violations():
    r = bench(2)
    assert (
        r["guardrail_violations"] == 0
        and r["optimizer"]["optimized_usd_per_call"]
        <= r["optimizer"]["naive_usd_per_call"]
    )
