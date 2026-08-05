export type Role = "user" | "assistant" | "system" | "tool";

export interface ToolCallFunction {
  name: string;
  arguments: any;
}

export interface ToolCallChunk {
  id?: string;
  function: ToolCallFunction;
}

export interface Message {
  role: Role;
  content: string;
  images?: string[];
  tool_calls?: ToolCallChunk[];
}

export interface Chat {
  id: string;
  title: string;
  model: string;
  messages: Message[];
  createdAt: number;
}

export interface HuggingFaceModel {
  repo_id: string;
  author: string;
  model_name: string;
  downloads: number;
  likes: number;
  tags: string[];
}

export interface HuggingFaceFile {
  filename: string;
  size: number;
  quant_label: string;
}

export interface OllamaModel {
  name: string;
  size: number;
  modified_at: string;
  details?: {
    parameter_size?: string;
    quantization_level?: string;
    family?: string;
  };
}

export interface ChatStreamChunk {
  delta: string;
  done: boolean;
  error?: string;
  ttft_ms?: number;
  tps?: number;
  tool_calls?: ToolCallChunk[];
}

export interface GpuInfo {
  name: string;
  vendor: string;
  vram_mb: number | null;
  driver?: string;
}

export interface HardwareInfo {
  os: string;
  os_version: string;
  cpu_brand: string;
  cpu_cores: number;
  total_ram_mb: number;
  gpus: GpuInfo[];
}

export type OllamaStatus =
  | { kind: "checking" }
  | { kind: "ok"; version: string }
  | { kind: "missing" }
  | { kind: "error"; message: string };

export interface GgufFile {
  name: string;
  size: number;
}
