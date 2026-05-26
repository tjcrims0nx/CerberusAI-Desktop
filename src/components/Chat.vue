<template>
  <div class="chat-component">
    <div
      class="msg-row"
      v-for="(m, i) in activeChat.messages"
      :key="i"
      :class="{ 'user-row': m.role === 'user' }"
    >
      <div v-if="m.role === 'assistant'" class="msg-avatar">
        C
      </div>
      <div class="msg-stack" :class="{ user: m.role === 'user', assistant: m.role === 'assistant' }">
        <div class="msg-bubble" :class="{ user: m.role === 'user', assistant: m.role === 'assistant' }">
          <div v-if="m.images && m.images.length > 0" class="msg-images">
            <img v-for="(img, idx) in m.images" :key="idx" :src="'data:image/jpeg;base64,' + img" class="msg-img" />
          </div>

          <div
            class="md-body"
            :class="{
              'streaming-pulse': streaming && i === activeChat.messages.length - 1 && m.role === 'assistant' &&
                stripThinkTags(streamingContent) === ''
            }"
          >
            <template v-if="streaming && i === activeChat.messages.length - 1 && m.role === 'assistant'">
              <span v-if="stripThinkTags(streamingContent) === ''" class="thinking-label">Thinking…</span>
              <div v-else class="md-body" v-html="renderMarkdown(stripThinkTags(streamingContent))"></div>
            </template>
            <template v-else>
              <div v-if="m.role === 'assistant'" class="md-body" v-html="renderMarkdown(m.content)"></div>
              <template v-else>{{ m.content }}</template>
            </template>
          </div>
        </div>

        <div v-if="m.role === 'assistant'" class="response-actions">
          <button class="copy-response-btn" @click="copyResponse(m.content, i)">
            {{ copiedIndex === i ? 'Copied' : 'Copy' }}
          </button>
          <div
            v-if="i === lastAssistantIndex && lastTtft !== null && !streaming"
            class="response-metrics"
          >
            <span>TTFT {{ lastTtft }}ms</span>
            <span v-if="lastTps">{{ lastTps.toFixed(1) }} tok/s</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';

const props = defineProps<{
  activeChat: any;
  streaming: boolean;
  streamingContent: string;
  lastTtft: number | null;
  lastTps: number | null;
  renderMarkdown: (text: string) => string;
  stripThinkTags: (text: string) => string;
}>();

const copiedIndex = ref<number | null>(null);

const lastAssistantIndex = computed(() => {
  for (let i = props.activeChat.messages.length - 1; i >= 0; i -= 1) {
    if (props.activeChat.messages[i].role === 'assistant') return i;
  }
  return -1;
});

async function copyResponse(content: string, index: number) {
  const text = props.stripThinkTags(content || '').trim();
  if (!text) return;
  await navigator.clipboard.writeText(text);
  copiedIndex.value = index;
  window.setTimeout(() => {
    if (copiedIndex.value === index) copiedIndex.value = null;
  }, 1400);
}
</script>
