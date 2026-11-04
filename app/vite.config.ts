import { defineConfig, loadEnv, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

// app/.env.local holds the dev server's engine URL and device token. Vite
// loads that file in every mode, so builds blank both values: the web app the
// relay serves (and the desktop app's dist/) must never carry them.
const DEV_ONLY = ['VITE_IRIS_URL', 'VITE_IRIS_TOKEN'];

// Fails the build if the dev token still ends up in the output.
function noDevSecrets(env: Record<string, string>): Plugin {
  const values = [env.VITE_IRIS_TOKEN].filter((v) => v && v.length >= 8);
  return {
    name: 'iris-no-dev-secrets',
    apply: 'build',
    generateBundle(_, bundle) {
      for (const file of Object.values(bundle)) {
        const text = file.type === 'chunk' ? file.code : String(file.source);
        if (values.some((v) => text.includes(v))) {
          this.error(`${file.fileName} contains the device token from app/.env.local`);
        }
      }
    },
  };
}

// Tauri expects a fixed dev port and serves the built files from dist/.
export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, process.cwd(), 'VITE_');
  // Variables already in the environment beat the .env files, so empty ones
  // blank the file's values for this build.
  if (command === 'build') for (const k of DEV_ONLY) process.env[k] = '';
  return {
    plugins: [svelte(), tailwindcss(), noDevSecrets(env)],
    clearScreen: false,
    server: { port: 1420, strictPort: true },
    envPrefix: ['VITE_', 'TAURI_ENV_'],
    build: {
      target: 'es2022',
      // The Windows webview (WebView2) is Chromium, so no legacy output needed.
      cssMinify: true,
      sourcemap: false,
    },
  };
});
