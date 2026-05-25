import os
from openai import OpenAI

client = OpenAI(
    api_key="cbs_live_C4L479axY27VAbGmNUNEcmWOqF8AaxgJ",
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
