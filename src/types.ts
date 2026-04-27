export type Role = "user" | "assistant" | "system";

export interface Message {
  role: Role;
  content: string;
}

export interface Chat {
  id: string;
  title: string;
  model: string;
  messages: Message[];
  createdAt: number;
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
