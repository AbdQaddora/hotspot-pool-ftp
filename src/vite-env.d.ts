/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TAURI_MOCK?: string;
}

declare const __APP_VERSION__: string;
