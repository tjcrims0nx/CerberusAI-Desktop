<template>
  <div class="animated-bg">
    <!-- Flare principal -->
    <div class="flare flare-1"></div>

    <!-- Flare secondaire -->
    <div class="flare flare-2"></div>

    <!-- Flare tertiaire -->
    <div class="flare flare-3"></div>

    <!-- Metal Grain -->
    <div class="grain"></div>

    <!-- Vignette -->
    <div class="vignette"></div>
  </div>
</template>

<style scoped>
.animated-bg {
  position: fixed;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  overflow: hidden;
  background-color: #050505;
}

.flare {
  position: absolute;
  filter: blur(80px);
}

@keyframes float-1 {
  0%, 100% { transform: rotate(-25deg) translate(0, 0) scale(1); opacity: 0.5; }
  33% { transform: rotate(-15deg) translate(80px, -120px) scale(1.1); opacity: 0.7; }
  66% { transform: rotate(-35deg) translate(-60px, 90px) scale(0.9); opacity: 0.4; }
}

@keyframes float-2 {
  0%, 100% { transform: rotate(-35deg) translate(0, 0) scale(1); opacity: 0.4; }
  33% { transform: rotate(-20deg) translate(-90px, -60px) scale(1.2); opacity: 0.6; }
  66% { transform: rotate(-45deg) translate(70px, 110px) scale(0.85); opacity: 0.3; }
}

@keyframes float-3 {
  0%, 100% { transform: rotate(-20deg) translate(0, 0) scale(1); opacity: 0.3; }
  33% { transform: rotate(-35deg) translate(100px, 80px) scale(1.3); opacity: 0.5; }
  66% { transform: rotate(-5deg) translate(-80px, -90px) scale(0.9); opacity: 0.2; }
}

.flare-1 {
  top: -10%;
  right: 10%;
  width: 800px;
  height: 800px;
  background: radial-gradient(ellipse at center, rgba(88, 28, 135, 0.45) 0%, rgba(147, 51, 234, 0.15) 40%, transparent 70%);
  animation: float-1 22s ease-in-out infinite;
}

.flare-2 {
  bottom: -10%;
  left: 10%;
  width: 900px;
  height: 700px;
  background: radial-gradient(ellipse at center, rgba(59, 7, 100, 0.5) 0%, rgba(168, 85, 247, 0.15) 50%, transparent 75%);
  animation: float-2 28s ease-in-out infinite;
}

.flare-3 {
  top: 30%;
  left: 40%;
  width: 700px;
  height: 600px;
  background: radial-gradient(ellipse at center, rgba(126, 34, 206, 0.35) 0%, rgba(192, 132, 252, 0.1) 60%, transparent 80%);
  animation: float-3 34s ease-in-out infinite;
  animation-delay: -5s;
}

.grain {
  position: absolute;
  inset: 0;
  opacity: 0.03;
  background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 256 256' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='noise'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23noise)'/%3E%3C/svg%3E");
}

.vignette {
  position: absolute;
  inset: 0;
  background: radial-gradient(ellipse at center, transparent 40%, rgba(0,0,0,0.85) 100%);
}
</style>
