import os
import sys
from openai import OpenAI

API_KEY = os.environ.get("CERBERUS_API_KEY")
if not API_KEY:
    print("Set CERBERUS_API_KEY before running this smoke test.", file=sys.stderr)
    sys.exit(1)

client = OpenAI(
    api_key=API_KEY,
    base_url="https://api.cerberusai.dev/v1"
)

def run_tests():
    print("TEST 1 - Fetching Models...")
    try:
        models = client.models.list()
        if not models.data:
            print("Failed: No models returned.")
            return
        model_id = models.data[0].id
        print(f"Found {len(models.data)} models. Using: {model_id}\n")
    except Exception as e:
        print("Failed to fetch models:", e)
        return

    print(f"TEST 2 - Non-streaming completion using {model_id}...")
    try:
        response = client.chat.completions.create(
            model=model_id,
            messages=[{"role": "user", "content": "Say hello!"}]
        )
        print("Success:", response.choices[0].message.content)
    except Exception as e:
        print("Failed:", e)

    print(f"\nTEST 3 - Streaming completion using {model_id}...")
    try:
        stream = client.chat.completions.create(
            model=model_id,
            messages=[{"role": "user", "content": "Count from 1 to 3."}],
            stream=True
        )
        print("Success: ", end="")
        for chunk in stream:
            if chunk.choices and chunk.choices[0].delta.content is not None:
                print(chunk.choices[0].delta.content, end="", flush=True)
        print()
    except Exception as e:
        print("Failed:", e)

if __name__ == "__main__":
    run_tests()
