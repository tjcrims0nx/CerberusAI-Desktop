import time
import requests
import json
import argparse
import sys

def benchmark_api(url, model, prompt, max_tokens=512, ttft_ceiling=None):
    headers = {
        "Content-Type": "application/json",
    }

    data = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": True,
        "max_tokens": max_tokens
    }

    start_time = time.time()
    first_token_time = None
    token_count = 0
    full_text = ""
    server_timings = None

    print(f"Sending request to {url}...")
    try:
        response = requests.post(url, headers=headers, json=data, stream=True)
        response.raise_for_status()

        for line in response.iter_lines():
            if not line:
                continue
            decoded_line = line.decode('utf-8')
            if not decoded_line.startswith("data: "):
                continue
            data_str = decoded_line[6:]
            if data_str == "[DONE]":
                break

            try:
                chunk = json.loads(data_str)
            except json.JSONDecodeError:
                continue

            # Capture llama.cpp authoritative timings from the final chunk if present.
            if isinstance(chunk.get("timings"), dict):
                server_timings = chunk["timings"]

            choices = chunk.get("choices") or []
            if not choices:
                continue

            delta = choices[0].get("delta", {}) or {}
            # llama.cpp emits content for normal tokens, reasoning_content for
            # thinking tokens. Both count as generated output for TTFT/TPS.
            content = delta.get("content")
            reasoning = delta.get("reasoning_content")

            if content or reasoning:
                if first_token_time is None:
                    first_token_time = time.time()
                if content:
                    full_text += content
                    sys.stdout.write(content)
                    sys.stdout.flush()
                token_count += 1

    except Exception as e:
        print(f"\nError: {e}")
        return

    end_time = time.time()
    print("\n\n" + "="*50)

    if first_token_time is None and server_timings is None:
        print("No tokens received.")
        return

    # Prefer llama.cpp's server-reported timings when available — they are exact.
    ttft = None
    tps = 0.0

    if server_timings:
        prompt_ms = server_timings.get("prompt_ms")
        predicted_per_second = server_timings.get("predicted_per_second")
        predicted_n = server_timings.get("predicted_n")
        predicted_ms = server_timings.get("predicted_ms")
        if isinstance(prompt_ms, (int, float)):
            ttft = prompt_ms / 1000.0
        if isinstance(predicted_per_second, (int, float)) and predicted_per_second > 0:
            tps = predicted_per_second
        elif (
            isinstance(predicted_n, (int, float))
            and isinstance(predicted_ms, (int, float))
            and predicted_ms > 0
        ):
            tps = predicted_n / (predicted_ms / 1000.0)
        if isinstance(predicted_n, (int, float)) and predicted_n > token_count:
            token_count = int(predicted_n)
        timings_source = "server"
    else:
        timings_source = "client"

    if ttft is None and first_token_time is not None:
        ttft = first_token_time - start_time
    if tps == 0.0 and first_token_time is not None:
        generation_time = end_time - first_token_time
        if generation_time > 0 and token_count > 1:
            tps = (token_count - 1) / generation_time

    if ttft is None:
        print("Could not determine TTFT.")
        return

    total_time = end_time - start_time

    print(f"Results ({timings_source} timings):")
    print(f"Model: {model}")
    print(f"Tokens generated: {token_count}")
    print(f"Time to First Token (TTFT): {ttft:.3f} seconds")
    print(f"Total time: {total_time:.3f} seconds")
    print(f"Tokens Per Second (TPS): {tps:.2f} tokens/s")

    # Skip posting if TTFT exceeds ceiling (cold start / model loading)
    if ttft_ceiling is not None and ttft > ttft_ceiling:
        print(f"TTFT {ttft:.1f}s exceeds ceiling {ttft_ceiling}s — skipping telemetry (likely cold start)")
        return

    # Post telemetry data to the dashboard
    dashboard_url = "http://127.0.0.1:3000/api/admin/benchmark"
    secret = "default_benchmark_secret"
    try:
        telemetry_response = requests.post(
            dashboard_url,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {secret}"
            },
            json={
                "model": model,
                "ttftMs": ttft * 1000,
                "tps": tps
            },
            timeout=5
        )
        if telemetry_response.status_code == 200:
            print(f"Successfully posted telemetry to dashboard (ID: {telemetry_response.json().get('id')})")
        else:
            print(f"Failed to post telemetry: {telemetry_response.status_code} {telemetry_response.text}")
    except Exception as e:
        print(f"Could not reach dashboard telemetry endpoint: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Benchmark API Server for TTFT and TPS")
    parser.add_argument("--url", type=str, default="http://127.0.0.1:8083/v1/chat/completions", help="API Endpoint URL")
    parser.add_argument("--model", type=str, default="default", help="Model name")
    parser.add_argument("--prompt", type=str, default="Explain the theory of relativity in simple terms in two paragraphs.", help="Prompt to send")
    parser.add_argument("--max-tokens", type=int, default=512, help="Max tokens to generate")
    parser.add_argument("--ttft-ceiling", type=float, default=None, help="Max TTFT in seconds — skip telemetry if exceeded (filters cold starts)")

    args = parser.parse_args()
    benchmark_api(args.url, args.model, args.prompt, args.max_tokens, args.ttft_ceiling)
