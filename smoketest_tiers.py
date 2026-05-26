import os
import time
from openai import OpenAI

TIERS = {
    "Premium": os.environ.get("CERBERUS_PREMIUM_API_KEY"),
    "Lite": os.environ.get("CERBERUS_LITE_API_KEY"),
}

BASE_URL = "https://api.cerberusai.dev/v1"

def run_tier_test(tier_name, api_key):
    print("=" * 60)
    print(f"TESTING TIER: {tier_name}")
    print("=" * 60)
    
    client = OpenAI(
        api_key=api_key,
        base_url=BASE_URL,
        timeout=15.0
    )

    print("TEST 1 - Fetching Models...")
    try:
        t0 = time.time()
        models = client.models.list()
        dt = time.time() - t0
        if not models.data:
            print("  [Failed]: No models returned.")
            return
        model_id = models.data[0].id
        print(f"  [Success] Found {len(models.data)} models in {dt:.2f}s. Using: {model_id}\n")
    except Exception as e:
        print("  [Failed] to fetch models:", e)
        return

    print(f"TEST 2 - Non-streaming completion using {model_id}...")
    try:
        t0 = time.time()
        response = client.chat.completions.create(
            model=model_id,
            messages=[{"role": "user", "content": "Say hello in one word."}]
        )
        dt = time.time() - t0
        print(f"  [Success] in {dt:.2f}s: {response.choices[0].message.content}")
    except Exception as e:
        print("  [Failed]:", e)

    print(f"\nTEST 3 - Streaming completion using {model_id}...")
    try:
        t0 = time.time()
        stream = client.chat.completions.create(
            model=model_id,
            messages=[{"role": "user", "content": "Count from 1 to 3."}],
            stream=True
        )
        print("  [Success]: ", end="")
        for chunk in stream:
            if chunk.choices and chunk.choices[0].delta.content is not None:
                print(chunk.choices[0].delta.content, end="", flush=True)
        dt = time.time() - t0
        print(f" (completed in {dt:.2f}s)\n")
    except Exception as e:
        print("  [Failed]:", e)
    
    print("\n")

if __name__ == "__main__":
    missing = [name for name, api_key in TIERS.items() if not api_key]
    if missing:
        names = ", ".join(f"CERBERUS_{name.upper()}_API_KEY" for name in missing)
        raise SystemExit(f"Set {names} before running this smoke test.")

    for tier_name, api_key in TIERS.items():
        run_tier_test(tier_name, api_key)
    print("ALL TESTS COMPLETE")
