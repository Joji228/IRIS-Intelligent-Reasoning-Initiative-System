// Security headers for the web app the relay serves (the phone-less way in).
//
// The web app keeps the device token and the Link keys in the browser, so the
// browser is told to run nothing but the app's own files: no inline scripts,
// no other origins, no framing (an approval button must not be clickable
// through someone else's page), and connections only to this relay.

export function securityHeaders(host: string, secure = true): Record<string, string> {
  const csp = [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self' data:",
    "media-src 'self' blob: data:",
    `connect-src 'self' ${secure ? 'wss' : 'ws'}://${host}`,
    "manifest-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
  ].join('; ');
  return {
    'Content-Security-Policy': csp,
    'X-Content-Type-Options': 'nosniff',
    'X-Frame-Options': 'DENY',
    'Referrer-Policy': 'no-referrer',
  };
}

// The frame visual cards run in (public/visual.html). A visual is a small page the
// model wrote, so it gets its own policy: inline scripts and styles yes, nothing from
// the network, and only the web app may frame it. The app loads it in a sandbox
// (an opaque origin), so it can't reach the app's storage either.
export function visualFrameHeaders(): Record<string, string> {
  const csp = [
    // Always an opaque origin, even if someone opens or frames it without a sandbox.
    'sandbox allow-scripts',
    "default-src 'none'",
    "script-src 'unsafe-inline'",
    "style-src 'unsafe-inline'",
    'img-src data: blob:',
    'font-src data:',
    'media-src data: blob:',
    "form-action 'none'",
    "base-uri 'none'",
    "frame-ancestors 'self'",
  ].join('; ');
  return {
    'Content-Security-Policy': csp,
    'X-Content-Type-Options': 'nosniff',
    'Referrer-Policy': 'no-referrer',
  };
}
