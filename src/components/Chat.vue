<template>
  <div class="chat-component">
    <div class="msg-row" v-for="(m, i) in activeChat.messages" :key="i" :class="{ 'user-row': m.role === 'user' }">
      <div v-if="m.role === 'assistant'" class="msg-avatar">
        C
      </div>
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
    </div>
    <div class="msg-row" v-if="lastTtft !== null && !streaming" style="margin-top: -12px; margin-bottom: 8px;">
      <div style="margin-left: 38px; display: flex; gap: 6px; opacity: 0.65;">
         <span class="model-tag">⚡ TTFT: {{ lastTtft }}ms</span>
         <span class="model-tag" v-if="lastTps">{{ lastTps.toFixed(1) }} tok/s</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  activeChat: any;
  streaming: boolean;
  streamingContent: string;
  lastTtft: number | null;
  lastTps: number | null;
  renderMarkdown: (text: string) => string;
  stripThinkTags: (text: string) => string;
}>();
</script>
