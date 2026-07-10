// Whether the app runs on a mobile OS (Android/iOS), as opposed to a desktop
// Tauri window. This is deliberately platform-based, NOT viewport-width-based:
// the desktop window is only 480px wide, so a width breakpoint would wrongly
// flag desktop as mobile. The webview user agent is stable for the process
// lifetime, so we compute it once.
const MOBILE_UA = /android|iphone|ipad|ipod/i;

export const isMobilePlatform =
  typeof navigator !== "undefined" && MOBILE_UA.test(navigator.userAgent);

export function useIsMobilePlatform(): boolean {
  return isMobilePlatform;
}
