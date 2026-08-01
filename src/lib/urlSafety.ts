// Checking a link before opening it in the external browser.
//
// The markdown in a note is arbitrary text: it may have been pasted from the
// clipboard, come from the AI, or arrived in an imported .md. Handing such a string
// to openUrl() without checking the scheme is not acceptable — `javascript:` and
// `data:` are executable code, and `file:` grants access to the local filesystem.
//
// A separate pure module, because vitest covers pure ts only (the same approach as
// guard.ts and clipboardNote.ts).

// An allowlist rather than a blocklist: an unknown scheme is more safely treated as
// unsafe, or every new exotic scheme becomes a hole.
const SAFE_SCHEMES = ["http:", "https:", "mailto:"];

export function isSafeUrl(raw: string): boolean {
  const trimmed = raw.trim();
  if (!trimmed) return false;

  // A link with no scheme (example.com, /path, #anchor) is not an absolute URL and
  // there is nothing to open it in the external browser with. We treat it as unsafe,
  // but for a different reason: not "dangerous" but "nowhere to lead".
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return false;
  }

  // toLowerCase: `JavaScript:` is the same scheme as `javascript:`.
  return SAFE_SCHEMES.includes(url.protocol.toLowerCase());
}
