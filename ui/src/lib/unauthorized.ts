// Shared by the browser backend (raises it) and the app shell (reacts to it), kept apart from
// http.ts so the HTTP backend stays a lazily loaded chunk.

/** Raised when the server says the session is missing or expired. */
export class UnauthorizedError extends Error {
  constructor() {
    super('Sign in to continue');
  }
}

let handler: (() => void) | null = null;

/** The app registers this to show the sign-in screen when a session expires mid-use. */
export function setUnauthorizedHandler(fn: (() => void) | null) {
  handler = fn;
}

export function notifyUnauthorized() {
  handler?.();
}
