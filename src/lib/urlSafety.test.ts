import { describe, it, expect } from "vitest";
import { isSafeUrl } from "./urlSafety";

describe("isSafeUrl", () => {
  it("обычные http/https ссылки безопасны", () => {
    expect(isSafeUrl("https://example.com")).toBe(true);
    expect(isSafeUrl("http://example.com/a/b?x=1#frag")).toBe(true);
    expect(isSafeUrl("  https://example.com  ")).toBe(true); // with surrounding spaces
  });

  it("mailto разрешён — это ссылка наружу, а не код", () => {
    expect(isSafeUrl("mailto:a@b.com")).toBe(true);
  });

  // The very reason this module exists: the markdown in a note is arbitrary text (a
  // clipboard paste, an AI answer, an imported .md).
  it("javascript: заблокирован, включая маскировку регистром", () => {
    expect(isSafeUrl("javascript:alert(1)")).toBe(false);
    expect(isSafeUrl("JavaScript:alert(1)")).toBe(false);
    expect(isSafeUrl("JAVASCRIPT:alert(1)")).toBe(false);
  });

  it("data: и file: заблокированы", () => {
    expect(isSafeUrl("data:text/html,<script>alert(1)</script>")).toBe(false);
    expect(isSafeUrl("file:///etc/passwd")).toBe(false);
  });

  it("относительные ссылки и мусор — не открываем", () => {
    expect(isSafeUrl("example.com")).toBe(false); // no scheme
    expect(isSafeUrl("/local/path")).toBe(false);
    expect(isSafeUrl("#anchor")).toBe(false);
    expect(isSafeUrl("")).toBe(false);
    expect(isSafeUrl("   ")).toBe(false);
    expect(isSafeUrl("не ссылка вовсе")).toBe(false);
  });
});
