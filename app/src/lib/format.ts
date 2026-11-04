/** 1234 -> "1.2k", 1_500_000 -> "1.5M". */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${+(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${+(n / 1_000).toFixed(1)}k`;
  return String(Math.round(n));
}

/** Friendly names for model ids that hide their version. */
const MODEL_VERSIONS: Record<string, string> = {
  'deepseek-flash': 'DeepSeek V4.1 Flash',
  'deepseek-v4-pro': 'DeepSeek V4 Pro',
};

/** The model's friendly name, when its id hides it. */
export function modelVersion(id: string): string | undefined {
  return MODEL_VERSIONS[id];
}

/** A model's name with its id, e.g. "DeepSeek V4.1 Flash (deepseek-flash)". */
export function modelName(id: string): string {
  return MODEL_VERSIONS[id] ? `${MODEL_VERSIONS[id]} (${id})` : id;
}

/** Context windows of well-known models (tokens); mirrors the engine's list. */
export function knownContextWindow(model: string): number | null {
  const m = model.toLowerCase();
  if (m.startsWith('deepseek-flash') || m.startsWith('deepseek-v4') || m === 'deepseek-chat' || m === 'deepseek-reasoner')
    return 1_000_000;
  if (m.startsWith('gemini')) return 1_000_000;
  if (m.startsWith('gpt-5')) return 400_000;
  if (m.startsWith('claude')) return 200_000;
  if (m.startsWith('gpt-4o') || m.startsWith('gpt-4.1') || m.startsWith('o3') || m.startsWith('o4')) return 128_000;
  return null;
}
