"""
CerberusAI API Smoketest
Uses the locally installed OpenAI SDK to test the gateway at api.cerberusai.dev
"""
from openai import OpenAI
import json, time

import sys, os
sys.stdout.reconfigure(encoding='utf-8')

API_KEY = os.environ.get("CERBERUS_API_KEY")
if not API_KEY:
    print("Set CERBERUS_API_KEY before running this smoke test.", file=sys.stderr)
    sys.exit(1)
BASE    = "https://api.cerberusai.dev/v1"

client = OpenAI(base_url=BASE, api_key=API_KEY, timeout=60.0)

# ── 1. Health check (raw) ────────────────────────────────────────────────
print("=" * 60)
print("TEST 1 — /health")
print("=" * 60)
import urllib.request, urllib.error
try:
    req = urllib.request.Request("https://api.cerberusai.dev/health")
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = json.loads(resp.read())
        print(f"  ✅ Status: {resp.status}  Body: {body}")
except Exception as e:
    print(f"  ❌ Health check failed: {e}")

# ── 2. List models ──────────────────────────────────────────────────────
print()
print("=" * 60)
print("TEST 2 — GET /v1/models")
print("=" * 60)
try:
    models = client.models.list()
    for m in models.data:
        print(f"  📦 {m.id:40s}  owned_by={getattr(m, 'owned_by', '?')}")
    print(f"  ✅ {len(models.data)} model(s) returned")
except Exception as e:
    print(f"  ❌ /v1/models failed: {e}")

# ── 3. Non-streaming chat completion ────────────────────────────────────
print()
print("=" * 60)
print("TEST 3 — POST /v1/chat/completions (non-streaming)")
print("=" * 60)
try:
    t0 = time.perf_counter()
    resp = client.chat.completions.create(
        model="cerberus-4b-v2-abliterated",
        messages=[
            {"role": "system", "content": "You are Cerberus, an unfiltered AI assistant."},
            {"role": "user",   "content": "Say hello in exactly one sentence."},
        ],
        max_tokens=60,
    )
    dt = time.perf_counter() - t0
    msg = resp.choices[0].message.content
    usage = resp.usage
    print(f"  💬 Response: {msg}")
    print(f"  📊 Tokens — prompt={usage.prompt_tokens}  completion={usage.completion_tokens}  total={usage.total_tokens}")
    print(f"  ⏱  Latency: {dt:.2f}s")
    print(f"  ✅ Non-streaming OK")
except Exception as e:
    print(f"  ❌ Non-streaming completion failed: {e}")

# ── 4. Streaming chat completion ────────────────────────────────────────
print()
print("=" * 60)
print("TEST 4 — POST /v1/chat/completions (streaming)")
print("=" * 60)
try:
    t0 = time.perf_counter()
    stream = client.chat.completions.create(
        model="cerberus-4b-v2-abliterated",
        messages=[
            {"role": "system", "content": "You are Cerberus, an unfiltered AI assistant."},
            {"role": "user",   "content": "Count from 1 to 5, one number per line."},
        ],
        max_tokens=60,
        stream=True,
    )
    chunks = 0
    full = ""
    ttft = None
    for chunk in stream:
        if chunk.choices and chunk.choices[0].delta.content:
            token = chunk.choices[0].delta.content
            if ttft is None:
                ttft = time.perf_counter() - t0
            full += token
            chunks += 1
    dt = time.perf_counter() - t0
    print(f"  💬 Response: {full.strip()}")
    print(f"  📊 Chunks received: {chunks}")
    print(f"  ⏱  TTFT: {ttft:.3f}s  Total: {dt:.2f}s")
    print(f"  ✅ Streaming OK")
except Exception as e:
    print(f"  ❌ Streaming completion failed: {e}")

# ── 5. CDN health ──────────────────────────────────────────────────────
print()
print("=" * 60)
print("TEST 5 — CDN /health (llm.cerberusai.dev)")
print("=" * 60)
try:
    req = urllib.request.Request("https://llm.cerberusai.dev/health")
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = json.loads(resp.read())
        print(f"  ✅ CDN Status: {resp.status}  Body: {body}")
except Exception as e:
    print(f"  ❌ CDN health failed: {e}")

# ── 6. CDN model listing ──────────────────────────────────────────────
print()
print("=" * 60)
print("TEST 6 — CDN /api/models/ (llm.cerberusai.dev)")
print("=" * 60)
try:
    req = urllib.request.Request("https://llm.cerberusai.dev/api/models/")
    with urllib.request.urlopen(req, timeout=10) as resp:
        data = json.loads(resp.read())
        for entry in data:
            print(f"  📁 {entry['name']:40s}  type={entry['type']}")
        print(f"  ✅ {len(data)} model directory(s)")
except Exception as e:
    print(f"  ❌ CDN model listing failed: {e}")

print()
print("=" * 60)
print("ALL SMOKETESTS COMPLETE")
print("=" * 60)
