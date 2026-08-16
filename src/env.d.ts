/// <reference types="vite/client" />

// TypeScript 6 errors on a side-effect import of an untyped module (TS2882), so
// `import './style.css'` in main.ts needs the ambient declarations Vite ships
// (`declare module '*.css' {}` and friends) to be in scope.
