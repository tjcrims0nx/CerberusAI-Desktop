<template>
  <div v-if="!apiKeyVerified" class="key-gate">
    <div class="gate-aurora gate-aurora-one"></div>
    <div class="gate-aurora gate-aurora-two"></div>
    <div class="gate-grid"></div>
    <div class="key-card">
      <div class="key-logo">
        <img src="../assets/logo.png" class="key-logo-img" alt="Cerberus Logo" />
      </div>
      <p class="key-eyebrow">Local-first intelligence</p>
      <h1 class="key-title">CERBERUS</h1>
      <p class="key-sub">
        Enter your API key to unlock local chat, cloud model access, and authenticated MCP skills.
        <a href="https://access.cerberusai.dev" target="_blank" rel="noopener">Get one here.</a>
      </p>

      <form class="key-form" @submit.prevent="$emit('submitKey', apiKeyDraftLocal)">
        <input
          type="password"
          v-model="apiKeyDraftLocal"
          placeholder="cb_••••••••••••••••••••"
          autocomplete="off"
          spellcheck="false"
          :disabled="verifying"
          autofocus
        />
        <button type="submit" :disabled="verifying || !apiKeyDraftLocal.trim()">
          <span v-if="!verifying">UNLOCK</span>
          <span v-else>VERIFYING…</span>
        </button>
      </form>
      <p v-if="verifyError" class="key-error">{{ verifyError }}</p>

      <p class="key-foot">
        Stored in the encrypted local app database and sent only to Cerberus cloud endpoints that require authentication.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';

const props = defineProps<{
  apiKeyVerified: boolean;
  apiKeyDraft: string;
  verifying: boolean;
  verifyError: string;
}>();

const emit = defineEmits(['submitKey', 'update:apiKeyDraft']);

const apiKeyDraftLocal = ref(props.apiKeyDraft);

watch(apiKeyDraftLocal, (val) => {
  emit('update:apiKeyDraft', val);
});

watch(() => props.apiKeyDraft, (val) => {
  apiKeyDraftLocal.value = val;
});
</script>

<style scoped>
.key-gate {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  overflow: hidden;
  padding: 24px;
  background:
    radial-gradient(circle at 18% 12%, rgba(220, 38, 38, 0.24), transparent 29%),
    radial-gradient(circle at 78% 18%, rgba(124, 58, 237, 0.22), transparent 30%),
    radial-gradient(circle at 50% 88%, rgba(20, 184, 166, 0.13), transparent 34%),
    linear-gradient(145deg, #07070b 0%, #111018 48%, #050506 100%);
  perspective: 1200px;
}

.gate-grid {
  position: absolute;
  inset: 0;
  opacity: 0.24;
  background-image:
    linear-gradient(rgba(255, 255, 255, 0.045) 1px, transparent 1px),
    linear-gradient(90deg, rgba(255, 255, 255, 0.045) 1px, transparent 1px);
  background-size: 54px 54px;
  mask-image: radial-gradient(circle at center, black 0%, transparent 72%);
  animation: grid-drift 14s linear infinite;
}

.gate-aurora {
  position: absolute;
  width: 44vmax;
  aspect-ratio: 1;
  border-radius: 50%;
  filter: blur(46px);
  opacity: 0.36;
  mix-blend-mode: screen;
  animation: aurora-float 10s ease-in-out infinite alternate;
}

.gate-aurora-one {
  left: -12vmax;
  top: -8vmax;
  background: conic-gradient(from 120deg, #dc2626, #7c3aed, #f97316, #dc2626);
}

.gate-aurora-two {
  right: -14vmax;
  bottom: -12vmax;
  background: conic-gradient(from 220deg, #14b8a6, #7c3aed, #be123c, #14b8a6);
  animation-delay: -3s;
}

.key-card {
  position: relative;
  z-index: 2;
  width: min(100%, 470px);
  padding: 38px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 24px;
  background:
    linear-gradient(145deg, rgba(42, 42, 56, 0.92) 0%, rgba(13, 13, 20, 0.88) 52%, rgba(5, 5, 9, 0.92) 100%);
  box-shadow:
    0 36px 90px rgba(0, 0, 0, 0.66),
    0 14px 34px rgba(0, 0, 0, 0.42),
    -14px -12px 44px rgba(124, 58, 237, 0.12),
    14px 12px 44px rgba(220, 38, 38, 0.1),
    inset 0 2px 0 rgba(255, 255, 255, 0.18),
    inset 0 -2px 0 rgba(0, 0, 0, 0.65),
    inset 2px 0 0 rgba(255, 255, 255, 0.05),
    inset -2px 0 0 rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(22px);
  text-align: center;
  transform-style: preserve-3d;
  animation: gate-in 520ms cubic-bezier(0.2, 0.9, 0.2, 1) both;
  transition: transform 240ms ease, box-shadow 240ms ease, border-color 240ms ease;
}

.key-card::before {
  content: "";
  position: absolute;
  inset: 7px;
  border-radius: inherit;
  pointer-events: none;
  opacity: 1;
  background:
    linear-gradient(145deg, rgba(255, 255, 255, 0.1), transparent 36%, rgba(0, 0, 0, 0.28) 100%);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.12),
    inset 0 -1px 0 rgba(0, 0, 0, 0.5);
}

.key-card::after {
  content: "";
  position: absolute;
  left: 16%;
  right: 16%;
  top: 0;
  height: 1px;
  border-radius: inherit;
  pointer-events: none;
  opacity: 0.8;
  background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.5), transparent);
}

.key-card:hover {
  transform: rotateX(1.8deg) rotateY(-2.4deg) translateY(-4px);
  border-color: rgba(248, 113, 113, 0.28);
  box-shadow:
    0 44px 110px rgba(0, 0, 0, 0.7),
    0 18px 42px rgba(0, 0, 0, 0.48),
    -18px -14px 52px rgba(124, 58, 237, 0.16),
    18px 14px 52px rgba(220, 38, 38, 0.14),
    inset 0 2px 0 rgba(255, 255, 255, 0.2),
    inset 0 -2px 0 rgba(0, 0, 0, 0.68);
}

.key-logo {
  width: 84px;
  height: 84px;
  display: grid;
  place-items: center;
  margin: 0 auto 18px;
  border-radius: 24px;
  background: linear-gradient(145deg, rgba(220, 38, 38, 0.44), rgba(124, 58, 237, 0.3));
  box-shadow:
    0 20px 42px rgba(0, 0, 0, 0.45),
    0 0 34px rgba(220, 38, 38, 0.24),
    inset 0 2px 0 rgba(255, 255, 255, 0.18),
    inset 0 -3px 0 rgba(0, 0, 0, 0.46);
  transform: translateZ(18px);
}

.key-logo-img {
  width: 62px;
  height: 62px;
  object-fit: contain;
  filter: drop-shadow(0 8px 18px rgba(248, 113, 113, 0.28));
}

.key-eyebrow {
  margin: 0 0 8px;
  color: #fb7185;
  font-size: 0.76rem;
  font-weight: 800;
  letter-spacing: 0.16em;
  text-transform: uppercase;
}

.key-title {
  margin: 0;
  color: white;
  font-size: clamp(2.35rem, 10vw, 4.2rem);
  line-height: 0.92;
  letter-spacing: 0;
  text-shadow: 0 0 30px rgba(220, 38, 38, 0.32);
  transform: translateZ(14px);
}

.key-sub {
  margin: 18px auto 24px;
  max-width: 34rem;
  color: rgba(255, 255, 255, 0.72);
  line-height: 1.55;
}

.key-sub a {
  color: #fda4af;
  font-weight: 800;
}

.key-form {
  display: grid;
  gap: 12px;
  transform: translateZ(10px);
}

.key-form input {
  width: 100%;
  min-height: 52px;
  padding: 0 16px;
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 14px;
  color: white;
  background: linear-gradient(180deg, rgba(0, 0, 0, 0.52), rgba(0, 0, 0, 0.34));
  box-shadow:
    inset 0 3px 8px rgba(0, 0, 0, 0.68),
    inset 0 1px 0 rgba(255, 255, 255, 0.06),
    0 1px 0 rgba(255, 255, 255, 0.08);
  font-size: 1rem;
  transition: border-color 0.2s ease, box-shadow 0.2s ease, background 0.2s ease;
}

.key-form input:focus {
  outline: none;
  border-color: rgba(248, 113, 113, 0.64);
  background: rgba(0, 0, 0, 0.48);
  box-shadow: 0 0 0 4px rgba(248, 113, 113, 0.12), 0 0 32px rgba(124, 58, 237, 0.22);
}

.key-form button {
  min-height: 52px;
  border: 1px solid rgba(248, 113, 113, 0.46);
  border-radius: 14px;
  color: white;
  cursor: pointer;
  font-weight: 900;
  letter-spacing: 0.08em;
  background: linear-gradient(135deg, #dc2626, #7c3aed);
  box-shadow:
    0 18px 40px rgba(220, 38, 38, 0.28),
    0 8px 0 rgba(0, 0, 0, 0.28),
    inset 0 2px 0 rgba(255, 255, 255, 0.2),
    inset 0 -3px 0 rgba(0, 0, 0, 0.28);
  transition: transform 0.2s ease, box-shadow 0.2s ease, filter 0.2s ease;
}

.key-form button:hover:not(:disabled) {
  transform: translateY(-3px);
  filter: brightness(1.08);
  box-shadow: 0 22px 52px rgba(124, 58, 237, 0.3), 0 0 26px rgba(248, 113, 113, 0.2);
}

.key-form button:active:not(:disabled) {
  transform: translateY(1px);
  box-shadow:
    0 10px 26px rgba(220, 38, 38, 0.22),
    0 3px 0 rgba(0, 0, 0, 0.28),
    inset 0 2px 6px rgba(0, 0, 0, 0.25);
}

.key-form button:disabled {
  cursor: not-allowed;
  opacity: 0.48;
}

.key-error {
  margin: 14px 0 0;
  color: #fecaca;
  font-weight: 700;
}

.key-foot {
  margin: 22px 0 0;
  color: rgba(255, 255, 255, 0.52);
  font-size: 0.82rem;
  line-height: 1.5;
}

@keyframes gate-in {
  from {
    opacity: 0;
    transform: translateY(18px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

@keyframes aurora-float {
  from {
    transform: translate3d(0, 0, 0) rotate(0deg);
  }
  to {
    transform: translate3d(4vmax, 3vmax, 0) rotate(16deg);
  }
}

@keyframes grid-drift {
  from {
    background-position: 0 0;
  }
  to {
    background-position: 54px 54px;
  }
}

@media (max-width: 560px) {
  .key-card {
    padding: 28px 20px;
    border-radius: 20px;
  }
}
</style>
