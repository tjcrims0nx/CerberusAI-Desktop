import { defineStore } from "pinia";
import { ref } from "vue";
import type { Chat, HardwareInfo, OllamaStatus } from "../types";

export const useAppStore = defineStore("app", () => {
  const selectedModel = ref<string>("");
  const cloudStatus = ref<OllamaStatus>({ kind: "checking" });
  const localStatus = ref<{ running: boolean; version?: string; error?: string }>({ running: false });
  const hardware = ref<HardwareInfo | null>(null);

  const cpuUsage = ref<number>(2);
  const ramUsage = ref<number>(25);
  const vramUsage = ref<number>(10);

  const streaming = ref<boolean>(false);
  const updating = ref<boolean>(false);
  const updateInfo = ref<{ current: string; latest: string; available: boolean } | null>(null);
  const appVersion = ref<string>("");

  const chats = ref<Chat[]>([]);
  const activeId = ref<string | null>(null);

  const apiKey = ref<string>("");
  const apiKeyVerified = ref<boolean>(false);

  return {
    selectedModel,
    cloudStatus,
    localStatus,
    hardware,
    cpuUsage,
    ramUsage,
    vramUsage,
    streaming,
    updating,
    updateInfo,
    appVersion,
    chats,
    activeId,
    apiKey,
    apiKeyVerified
  };
});
