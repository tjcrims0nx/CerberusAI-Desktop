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

export interface AllowedModel {
  id: string;
  description: string;
  quants: string;
  /**
   * Per-quant on-disk file size in bytes, parsed from the CDN listing.
   * Empty if the listing couldn't be fetched. Used to flag quants that
   * won't fit on the user's GPU before they pull.
   */
  quant_sizes?: Record<string, number>;
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
