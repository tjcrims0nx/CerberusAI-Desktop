import os
import sys
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Optional

# We use AirLLM to load massive models layer by layer
try:
    from airllm import AutoModel
except ImportError:
    # Fallback/mock for when the environment is building
    AutoModel = None

app = FastAPI(title="HELIX AirLLM Sidecar")

class ChatMessage(BaseModel):
    role: str
    content: str

class ChatRequest(BaseModel):
    model: str
    messages: List[ChatMessage]
    temperature: Optional[float] = 0.7
    max_tokens: Optional[int] = 1024

# Cache the loaded model
_current_model_path = None
_model_instance = None

@app.post("/v1/chat/completions")
async def chat_completions(req: ChatRequest):
    global _current_model_path, _model_instance

    if AutoModel is None:
        raise HTTPException(status_code=500, detail="airllm is not installed in the sidecar environment.")

    model_path = req.model # In HELIX, the frontend passes the path to the model

    if _current_model_path != model_path:
        # Load new model (AirLLM automatically offloads layer by layer from disk)
        print(f"Loading AirLLM model from {model_path}...")
        _model_instance = AutoModel.from_pretrained(model_path)
        _current_model_path = model_path

    # Convert messages to prompt string (naive for now, can be improved with chat templates)
    prompt = ""
    for m in req.messages:
        prompt += f"<{m.role}>\n{m.content}\n</{m.role}>\n"
    prompt += "<assistant>\n"

    input_text = [prompt]

    # Run inference
    import torch
    input_ids = _model_instance.tokenizer(input_text, return_tensors="pt", return_attention_mask=False).input_ids

    generation_output = _model_instance.generate(
        input_ids.cuda() if torch.cuda.is_available() else input_ids,
        max_new_tokens=req.max_tokens,
        use_cache=True,
        return_dict_in_generate=True
    )

    out_tokens = generation_output.sequences[0]
    out_text = _model_instance.tokenizer.decode(out_tokens, skip_special_tokens=True)

    # Strip the prompt from the output
    response = out_text[len(prompt):].strip()

    return {
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": response
                }
            }
        ]
    }

if __name__ == "__main__":
    # If run as sidecar by Tauri, we can accept port as arg
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 11435
    uvicorn.run(app, host="127.0.0.1", port=port)
