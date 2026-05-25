<template>
  <div class="horizon-empty">
    <div class="horizon-welcome-wrapper" style="animation: liquid-enter 0.8s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;">
      <div style="display: inline-flex; align-items: center; gap: 8px; background: linear-gradient(135deg, rgba(88, 28, 135, 0.3) 0%, rgba(0,0,0,0.5) 100%); border: 1px solid rgba(168,85,247,0.2); padding: 6px 16px; border-radius: 50px; font-size: 0.7rem; font-weight: 800; letter-spacing: 2px; color: #d8b4fe; margin-bottom: 2rem; box-shadow: 0 0 20px rgba(88, 28, 135, 0.4), inset 0 1px 0 rgba(255,255,255,0.05);">
        <span style="width: 8px; height: 8px; border-radius: 50%; background: #a855f7; box-shadow: 0 0 12px #a855f7;"></span>
        SYSTEM READY
      </div>

      <h2 style="font-size: 3rem; font-weight: 900; letter-spacing: -1px; color: #fff; margin-bottom: 0.5rem; text-shadow: 0 4px 20px rgba(0,0,0,0.5);">Good morning, Admin</h2>
      <p style="font-size: 1.1rem; color: rgba(255,255,255,0.6); margin-bottom: 0.5rem;">What would you like to do?</p>
      <p style="font-size: 0.85rem; color: rgba(255,255,255,0.4);">Everything stays on this computer unless you say otherwise.</p>
    </div>

    <!-- Empty state action for model pull -->
    <div v-if="localStatus.running && models.length === 0" style="text-align: center; margin-bottom: 2rem;">
      <p style="color: rgba(255,255,255,0.5); font-size: 0.9rem; margin-bottom: 1rem;">No local models found.</p>
      <button class="btn-metal-dark" style="padding: 12px 24px; border-radius: 8px; font-weight: 800; letter-spacing: 2px;" @click="$emit('openFileManager')">OPEN MODEL MANAGER TO PULL OR IMPORT</button>
    </div>

    <div class="horizon-grid" style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1.5rem; width: 100%; max-width: 700px; animation: liquid-enter 1s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;">
      <button class="card-metal" style="padding: 1.8rem; border-radius: 20px; text-align: left; display: flex; flex-direction: column;" @click="$emit('useSuggestion', 'Ask a question: ')">
        <div style="background: linear-gradient(135deg, rgba(88, 28, 135, 0.4), rgba(0,0,0,0.5)); width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; margin-bottom: 1.2rem; border: 1px solid rgba(168, 85, 247, 0.2); box-shadow: 0 4px 15px rgba(0,0,0,0.4);">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#d8b4fe" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path></svg>
        </div>
        <h3 style="font-size: 1rem; font-weight: 700; color: #fff; margin-bottom: 0.25rem;">Ask a question</h3>
        <p style="font-size: 0.8rem; color: rgba(255,255,255,0.4);">A clear, direct answer.</p>
      </button>
      <button class="card-metal" style="padding: 1.8rem; border-radius: 20px; text-align: left; display: flex; flex-direction: column;" @click="$emit('useSuggestion', 'Explain this text simply: ')">
        <div style="background: linear-gradient(135deg, rgba(88, 28, 135, 0.4), rgba(0,0,0,0.5)); width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; margin-bottom: 1.2rem; border: 1px solid rgba(168, 85, 247, 0.2); box-shadow: 0 4px 15px rgba(0,0,0,0.4);">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#d8b4fe" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>
        </div>
        <h3 style="font-size: 1rem; font-weight: 700; color: #fff; margin-bottom: 0.25rem;">Understand a text</h3>
        <p style="font-size: 0.8rem; color: rgba(255,255,255,0.4);">Simple explanation, step by step.</p>
      </button>
      <button class="card-metal" style="padding: 1.8rem; border-radius: 20px; text-align: left; display: flex; flex-direction: column;" @click="$emit('useSuggestion', 'Help me write: ')">
        <div style="background: linear-gradient(135deg, rgba(88, 28, 135, 0.4), rgba(0,0,0,0.5)); width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; margin-bottom: 1.2rem; border: 1px solid rgba(168, 85, 247, 0.2); box-shadow: 0 4px 15px rgba(0,0,0,0.4);">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#d8b4fe" stroke-width="2"><path d="M12 20h9"></path><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"></path></svg>
        </div>
        <h3 style="font-size: 1rem; font-weight: 700; color: #fff; margin-bottom: 0.25rem;">Writing help</h3>
        <p style="font-size: 0.8rem; color: rgba(255,255,255,0.4);">Rewrite, improve, or draft.</p>
      </button>
      <button class="card-metal" style="padding: 1.8rem; border-radius: 20px; text-align: left; display: flex; flex-direction: column;" @click="$emit('useSuggestion', 'Analyze the project at this path: ')">
        <div style="background: linear-gradient(135deg, rgba(88, 28, 135, 0.4), rgba(0,0,0,0.5)); width: 44px; height: 44px; display: flex; align-items: center; justify-content: center; border-radius: 12px; margin-bottom: 1.2rem; border: 1px solid rgba(168, 85, 247, 0.2); box-shadow: 0 4px 15px rgba(0,0,0,0.4);">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#d8b4fe" stroke-width="2"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        </div>
        <h3 style="font-size: 1rem; font-weight: 700; color: #fff; margin-bottom: 0.25rem;">Analyze a project</h3>
        <p style="font-size: 0.8rem; color: rgba(255,255,255,0.4);">Optional. For a local folder.</p>
      </button>
    </div>

    <div class="horizon-footer-hint">
      <span>Local assistant. No automatic sharing.</span>
      <span class="advanced-link" @click="$emit('openPluginManager')">ADVANCED CENTER →</span>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  localStatus: { running: boolean };
  models: any[];
}>();

const emit = defineEmits(['useSuggestion', 'openFileManager', 'openPluginManager']);
</script>
