/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_IRIS_URL?: string;
  readonly VITE_IRIS_TOKEN?: string;
  /** '1' in the web app the relay serves (`vite build --mode link`). */
  readonly VITE_IRIS_LINK?: string;
}
