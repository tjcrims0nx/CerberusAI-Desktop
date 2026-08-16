<template>
  <div class="chat-component" @click="handleChatClick">
    <template v-for="(m, i) in activeChat.messages" :key="i">
    <div
      v-if="isVisible(m, i)"
      class="msg-row"
      :class="{ 'user-row': m.role === 'user' }"
    >
      <div v-if="m.role === 'assistant'" class="msg-avatar">
        <img src="/helix-logo.png" alt="HELIX" class="msg-avatar-logo" />
      </div>
      <div class="msg-stack" :class="{ user: m.role === 'user', assistant: m.role === 'assistant' }">
        <div class="msg-bubble" :class="{ user: m.role === 'user', assistant: m.role === 'assistant' }">
          <div v-if="m.images && m.images.length > 0" class="msg-images">
            <img v-for="(img, idx) in m.images" :key="idx" :src="'data:image/jpeg;base64,' + img" class="msg-img" />
          </div>

          <div v-if="m.files && m.files.length > 0" class="msg-files">
            <div v-for="(f, idx) in m.files" :key="idx" class="msg-file">
              <button
                class="msg-file-pill"
                :class="{ open: expandedFile === i + ':' + idx }"
                :title="expandedFile === i + ':' + idx ? 'Hide contents' : 'Show contents'"
                @click.stop="toggleFile(i, idx)"
              >
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>
                <span class="msg-file-name">{{ f.name }}</span>
                <span class="msg-file-meta">{{ fileMeta(f.content) }}</span>
              </button>
              <pre v-if="expandedFile === i + ':' + idx" class="msg-file-body">{{ f.content }}</pre>
            </div>
          </div>

          <div v-if="m.tool_calls && m.tool_calls.length" class="tool-call-list">
            <span v-for="(tc, ti) in m.tool_calls" :key="ti" class="tool-call-chip">
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"></path></svg>
              {{ tc.function?.name || 'tool' }}
            </span>
          </div>

          <div
            v-if="hasBody(m, i)"
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

        <div class="response-actions" :class="{ 'user-actions': m.role === 'user' }">
          <button v-if="(m.content || '').trim()" class="copy-response-btn" @click="copyResponse(m.content, i)">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 4px; display: inline-block; vertical-align: -1px;"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
            {{ copiedIndex === i ? 'Copied ✓' : 'Copy' }}
          </button>
          <div
            v-if="m.role === 'assistant' && i === lastAssistantIndex && lastTtft !== null && !streaming"
            class="response-metrics"
          >
            <span>TTFT {{ lastTtft }}ms</span>
            <span v-if="lastTps">{{ lastTps.toFixed(1) }} tok/s</span>
          </div>
        </div>
      </div>
    </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { Chat } from '../types';

const props = defineProps<{
  // Typed rather than `any` because vue-tsc widens a `v-for` index over `any`
  // to `string | number`, which then fails to match the `i: number` handlers.
  activeChat: Chat;
  streaming: boolean;
  streamingContent: string;
  lastTtft: number | null;
  lastTps: number | null;
  renderMarkdown: (text: string) => string;
  stripThinkTags: (text: string) => string;
}>();

const copiedIndex = ref<number | null>(null);
// "<messageIndex>:<fileIndex>" of the one attachment whose body is expanded.
const expandedFile = ref<string | null>(null);

function toggleFile(msgIndex: number, fileIndex: number) {
  const key = `${msgIndex}:${fileIndex}`;
  expandedFile.value = expandedFile.value === key ? null : key;
}

/** Line and size summary shown on the pill, so the file isn't opaque. */
function fileMeta(content: string): string {
  const lines = content ? content.split('\n').length : 0;
  const bytes = content ? new Blob([content]).size : 0;
  const size = bytes >= 1024 * 1024
    ? `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    : bytes >= 1024
      ? `${(bytes / 1024).toFixed(1)} KB`
      : `${bytes} B`;
  return `${lines} lines · ${size}`;
}

/** True while this row is the one the stream is actively writing into. */
function isStreamingRow(m: any, i: number) {
  return props.streaming && i === props.activeChat.messages.length - 1 && m.role === 'assistant';
}

/**
 * Raw `tool` results are context for the model, not chat content — showing them
 * produced bare bubbles with nothing but a Copy button. The tool names are
 * surfaced as chips on the assistant turn that requested them instead. Empty
 * assistant turns are hidden too: `send()` pushes a placeholder before the
 * stream starts and one per tool round, and a round that returns only tool
 * calls leaves its placeholder permanently blank.
 */
function isVisible(m: any, i: number) {
  if (m.role === 'tool') return false;
  if (isStreamingRow(m, i)) return true;
  if ((m.content || '').trim()) return true;
  // An image-only message has no text but is still a real turn.
  if (m.images && m.images.length) return true;
  // Same for a message that is nothing but attached files.
  if (m.files && m.files.length) return true;
  return m.role === 'assistant' && !!(m.tool_calls && m.tool_calls.length);
}

/** Whether to render a markdown body, as opposed to tool chips alone. */
function hasBody(m: any, i: number) {
  return isStreamingRow(m, i) || !!(m.content || '').trim();
}

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

async function handleChatClick(event: MouseEvent) {
  const target = event.target as HTMLElement;
  const copyBtn = target.closest('.copy-code-btn') as HTMLButtonElement | null;
  const saveBtn = target.closest('.save-code-btn') as HTMLButtonElement | null;

  if (copyBtn) {
    event.preventDefault();
    event.stopPropagation();
    const container = copyBtn.closest('.code-block-container');
    const codeEl = container?.querySelector('code');
    if (!codeEl) return;
    const text = codeEl.textContent || '';
    await navigator.clipboard.writeText(text);

    const span = copyBtn.querySelector('span');
    if (span) {
      const orig = span.textContent;
      span.textContent = 'Copied ✓';
      copyBtn.classList.add('copied');
      setTimeout(() => {
        span.textContent = orig;
        copyBtn.classList.remove('copied');
      }, 1500);
    }
    return;
  }

  if (saveBtn) {
    event.preventDefault();
    event.stopPropagation();
    const container = saveBtn.closest('.code-block-container');
    const codeEl = container?.querySelector('code');
    if (!codeEl) return;
    const text = codeEl.textContent || '';
    const lang = container?.getAttribute('data-lang') || 'file';
    const defaultFilename = container?.getAttribute('data-filename') || `file.${lang}`;

    const span = saveBtn.querySelector('span');
    try {
      const filePath = await save({
        defaultPath: defaultFilename,
      });
      if (filePath) {
        await invoke('save_text_file', { path: filePath, content: text });
        if (span) {
          const orig = span.textContent;
          span.textContent = 'Saved ✓';
          saveBtn.classList.add('saved');
          setTimeout(() => {
            span.textContent = orig;
            saveBtn.classList.remove('saved');
          }, 1500);
        }
      }
    } catch {
      const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = defaultFilename;
      a.click();
      URL.revokeObjectURL(url);
      if (span) {
        const orig = span.textContent;
        span.textContent = 'Saved ✓';
        saveBtn.classList.add('saved');
        setTimeout(() => {
          span.textContent = orig;
          saveBtn.classList.remove('saved');
        }, 1500);
      }
    }
  }
}
</script>
