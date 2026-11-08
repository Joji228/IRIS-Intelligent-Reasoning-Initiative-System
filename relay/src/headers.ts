// Security headers for the web app the relay serves (the phone-less way in).
//
// The web app keeps the device token and the Link keys in the browser, so the
// browser is told to run nothing but the app's own files: no inline scripts,
// no other origins, no framing (an approval button must not be clickable
// through someone else's page), and connections only to this relay.

export function securityHeaders(host: string): Record<string, string> {
  const csp = [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self' data:",
    "media-src 'self' blob: data:",
    `connect-src 'self' wss://${host}`,
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
